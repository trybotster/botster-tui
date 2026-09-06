//! TUI-owned mapping from Crossterm events to Core terminal input scheme 2.
//!
//! Core owns the neutral key, mouse, and mode tables and the binary codec.
//! This module owns the Crossterm side only: which `KeyEvent` becomes which
//! `TerminalKey`, which `MouseEvent` becomes which MOUSE frame, and the
//! client-side in-flight window (32 operations per route, 2 MiB queued).
//!
//! A TUI has no pixel geometry. RESIZE and MOUSE frames carry cell geometry
//! and zero pixel geometry; the worker maps cells itself.

use std::collections::VecDeque;

use botster_terminal_protocol_client::{
    TerminalInputCommand, TerminalKey, TerminalKeyAction, TerminalMouseAction, TerminalMouseButton,
    mode_bits, terminal_mods,
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MediaKeyCode, ModifierKeyCode, MouseButton,
    MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

/// In-flight input operations per route before the client queues locally.
pub const MAX_IN_FLIGHT_OPERATIONS: usize = 32;
/// Locally queued encoded input bytes per route before input is rejected.
pub const MAX_QUEUED_INPUT_BYTES: usize = 2 * 1024 * 1024;

/// Kit terminal mouse-passthrough bits derived from Core MODES bits.
///
/// Kit bit 1 = DEC 1000 (button press tracking), bit 2 = DEC 1003 (any
/// motion), bit 4 = DEC 1002 (button motion), bit 8 = SGR encoding.
#[must_use]
pub fn kit_mouse_bits(mode: u32) -> u8 {
    let mut bits = 0_u8;
    if mode & mode_bits::MOUSE_NORMAL != 0 {
        bits |= 1;
    }
    if mode & mode_bits::MOUSE_ANY != 0 {
        bits |= 2;
    }
    if mode & mode_bits::MOUSE_BUTTON != 0 {
        bits |= 4;
    }
    if mode & mode_bits::MOUSE_SGR != 0 {
        bits |= 8;
    }
    bits
}

/// Whether any mouse tracking mode is enabled.
#[must_use]
pub fn mouse_tracking_enabled(mode: u32) -> bool {
    kit_mouse_bits(mode) & 0b0000_0111 != 0
}

fn mods_from_crossterm(modifiers: KeyModifiers) -> u16 {
    let mut mods = 0_u16;
    if modifiers.contains(KeyModifiers::SHIFT) {
        mods |= terminal_mods::SHIFT;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        mods |= terminal_mods::CTRL;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        mods |= terminal_mods::ALT;
    }
    if modifiers.intersects(KeyModifiers::SUPER | KeyModifiers::META | KeyModifiers::HYPER) {
        mods |= terminal_mods::SUPER;
    }
    mods
}

/// Physical-key mapping for one character on a US layout.
///
/// Returns the W3C code, the unshifted character, and whether Shift was
/// consumed to produce the character.
fn char_key(character: char) -> Option<(TerminalKey, char, bool)> {
    let lower = character.to_ascii_lowercase();
    let key = match lower {
        'a' => TerminalKey::KeyA,
        'b' => TerminalKey::KeyB,
        'c' => TerminalKey::KeyC,
        'd' => TerminalKey::KeyD,
        'e' => TerminalKey::KeyE,
        'f' => TerminalKey::KeyF,
        'g' => TerminalKey::KeyG,
        'h' => TerminalKey::KeyH,
        'i' => TerminalKey::KeyI,
        'j' => TerminalKey::KeyJ,
        'k' => TerminalKey::KeyK,
        'l' => TerminalKey::KeyL,
        'm' => TerminalKey::KeyM,
        'n' => TerminalKey::KeyN,
        'o' => TerminalKey::KeyO,
        'p' => TerminalKey::KeyP,
        'q' => TerminalKey::KeyQ,
        'r' => TerminalKey::KeyR,
        's' => TerminalKey::KeyS,
        't' => TerminalKey::KeyT,
        'u' => TerminalKey::KeyU,
        'v' => TerminalKey::KeyV,
        'w' => TerminalKey::KeyW,
        'x' => TerminalKey::KeyX,
        'y' => TerminalKey::KeyY,
        'z' => TerminalKey::KeyZ,
        _ => {
            return shifted_symbol_key(character)
                .or_else(|| unshifted_symbol_key(character).map(|key| (key, character, false)));
        }
    };
    let shifted = character.is_ascii_uppercase();
    Some((key, lower, shifted))
}

fn unshifted_symbol_key(character: char) -> Option<TerminalKey> {
    Some(match character {
        '0' => TerminalKey::Digit0,
        '1' => TerminalKey::Digit1,
        '2' => TerminalKey::Digit2,
        '3' => TerminalKey::Digit3,
        '4' => TerminalKey::Digit4,
        '5' => TerminalKey::Digit5,
        '6' => TerminalKey::Digit6,
        '7' => TerminalKey::Digit7,
        '8' => TerminalKey::Digit8,
        '9' => TerminalKey::Digit9,
        ' ' => TerminalKey::Space,
        '-' => TerminalKey::Minus,
        '=' => TerminalKey::Equal,
        '[' => TerminalKey::BracketLeft,
        ']' => TerminalKey::BracketRight,
        '\\' => TerminalKey::Backslash,
        ';' => TerminalKey::Semicolon,
        '\'' => TerminalKey::Quote,
        '`' => TerminalKey::Backquote,
        ',' => TerminalKey::Comma,
        '.' => TerminalKey::Period,
        '/' => TerminalKey::Slash,
        _ => return None,
    })
}

fn shifted_symbol_key(character: char) -> Option<(TerminalKey, char, bool)> {
    let (key, unshifted) = match character {
        ')' => (TerminalKey::Digit0, '0'),
        '!' => (TerminalKey::Digit1, '1'),
        '@' => (TerminalKey::Digit2, '2'),
        '#' => (TerminalKey::Digit3, '3'),
        '$' => (TerminalKey::Digit4, '4'),
        '%' => (TerminalKey::Digit5, '5'),
        '^' => (TerminalKey::Digit6, '6'),
        '&' => (TerminalKey::Digit7, '7'),
        '*' => (TerminalKey::Digit8, '8'),
        '(' => (TerminalKey::Digit9, '9'),
        '_' => (TerminalKey::Minus, '-'),
        '+' => (TerminalKey::Equal, '='),
        '{' => (TerminalKey::BracketLeft, '['),
        '}' => (TerminalKey::BracketRight, ']'),
        '|' => (TerminalKey::Backslash, '\\'),
        ':' => (TerminalKey::Semicolon, ';'),
        '"' => (TerminalKey::Quote, '\''),
        '~' => (TerminalKey::Backquote, '`'),
        '<' => (TerminalKey::Comma, ','),
        '>' => (TerminalKey::Period, '.'),
        '?' => (TerminalKey::Slash, '/'),
        _ => return None,
    };
    Some((key, unshifted, true))
}

fn function_key(number: u8) -> Option<TerminalKey> {
    Some(match number {
        1 => TerminalKey::F1,
        2 => TerminalKey::F2,
        3 => TerminalKey::F3,
        4 => TerminalKey::F4,
        5 => TerminalKey::F5,
        6 => TerminalKey::F6,
        7 => TerminalKey::F7,
        8 => TerminalKey::F8,
        9 => TerminalKey::F9,
        10 => TerminalKey::F10,
        11 => TerminalKey::F11,
        12 => TerminalKey::F12,
        13 => TerminalKey::F13,
        14 => TerminalKey::F14,
        15 => TerminalKey::F15,
        16 => TerminalKey::F16,
        17 => TerminalKey::F17,
        18 => TerminalKey::F18,
        19 => TerminalKey::F19,
        20 => TerminalKey::F20,
        21 => TerminalKey::F21,
        22 => TerminalKey::F22,
        23 => TerminalKey::F23,
        24 => TerminalKey::F24,
        _ => return None,
    })
}

fn media_key(media: MediaKeyCode) -> TerminalKey {
    match media {
        MediaKeyCode::Play | MediaKeyCode::Pause | MediaKeyCode::PlayPause => {
            TerminalKey::MediaPlayPause
        }
        MediaKeyCode::Stop => TerminalKey::MediaStop,
        MediaKeyCode::TrackNext => TerminalKey::MediaTrackNext,
        MediaKeyCode::TrackPrevious => TerminalKey::MediaTrackPrevious,
        MediaKeyCode::LowerVolume => TerminalKey::AudioVolumeDown,
        MediaKeyCode::RaiseVolume => TerminalKey::AudioVolumeUp,
        MediaKeyCode::MuteVolume => TerminalKey::AudioVolumeMute,
        MediaKeyCode::Reverse
        | MediaKeyCode::FastForward
        | MediaKeyCode::Rewind
        | MediaKeyCode::Record => TerminalKey::Unidentified,
    }
}

fn modifier_key(modifier: ModifierKeyCode) -> TerminalKey {
    match modifier {
        ModifierKeyCode::LeftShift | ModifierKeyCode::IsoLevel3Shift => TerminalKey::ShiftLeft,
        ModifierKeyCode::RightShift | ModifierKeyCode::IsoLevel5Shift => TerminalKey::ShiftRight,
        ModifierKeyCode::LeftControl => TerminalKey::ControlLeft,
        ModifierKeyCode::RightControl => TerminalKey::ControlRight,
        ModifierKeyCode::LeftAlt => TerminalKey::AltLeft,
        ModifierKeyCode::RightAlt => TerminalKey::AltRight,
        ModifierKeyCode::LeftSuper | ModifierKeyCode::LeftMeta | ModifierKeyCode::LeftHyper => {
            TerminalKey::MetaLeft
        }
        ModifierKeyCode::RightSuper | ModifierKeyCode::RightMeta | ModifierKeyCode::RightHyper => {
            TerminalKey::MetaRight
        }
    }
}

/// Map one Crossterm key event to a KEY command, or `None` for keys the
/// terminal never receives (for example `KeyCode::Null`).
#[must_use]
pub fn key_command(key: KeyEvent, operation_id: u64) -> Option<TerminalInputCommand> {
    let action = match key.kind {
        KeyEventKind::Press => TerminalKeyAction::Press,
        KeyEventKind::Repeat => TerminalKeyAction::Repeat,
        KeyEventKind::Release => TerminalKeyAction::Release,
    };
    let mut mods = mods_from_crossterm(key.modifiers);
    let mut consumed_mods = 0_u16;
    let mut unshifted_codepoint = 0_u32;
    let mut text = String::new();
    let text_mods = terminal_mods::CTRL | terminal_mods::ALT | terminal_mods::SUPER;
    let terminal_key = match key.code {
        KeyCode::Char(character) => {
            let (terminal_key, unshifted, shifted) =
                char_key(character).unwrap_or((TerminalKey::Unidentified, character, false));
            if shifted {
                mods |= terminal_mods::SHIFT;
                consumed_mods |= terminal_mods::SHIFT;
            }
            unshifted_codepoint = u32::from(unshifted);
            if mods & text_mods == 0 {
                text.push(character);
            }
            terminal_key
        }
        KeyCode::Backspace => TerminalKey::Backspace,
        KeyCode::Enter => TerminalKey::Enter,
        KeyCode::Left => TerminalKey::ArrowLeft,
        KeyCode::Right => TerminalKey::ArrowRight,
        KeyCode::Up => TerminalKey::ArrowUp,
        KeyCode::Down => TerminalKey::ArrowDown,
        KeyCode::Home => TerminalKey::Home,
        KeyCode::End => TerminalKey::End,
        KeyCode::PageUp => TerminalKey::PageUp,
        KeyCode::PageDown => TerminalKey::PageDown,
        KeyCode::Tab => TerminalKey::Tab,
        KeyCode::BackTab => {
            mods |= terminal_mods::SHIFT;
            TerminalKey::Tab
        }
        KeyCode::Delete => TerminalKey::Delete,
        KeyCode::Insert => TerminalKey::Insert,
        KeyCode::F(number) => function_key(number)?,
        KeyCode::Null => return None,
        KeyCode::Esc => TerminalKey::Escape,
        KeyCode::CapsLock => TerminalKey::CapsLock,
        KeyCode::ScrollLock => TerminalKey::ScrollLock,
        KeyCode::NumLock => TerminalKey::NumLock,
        KeyCode::PrintScreen => TerminalKey::PrintScreen,
        KeyCode::Pause => TerminalKey::Pause,
        KeyCode::Menu => TerminalKey::ContextMenu,
        KeyCode::KeypadBegin => TerminalKey::Numpad5,
        KeyCode::Media(media) => media_key(media),
        KeyCode::Modifier(modifier) => modifier_key(modifier),
    };
    Some(TerminalInputCommand::Key {
        operation_id,
        action,
        key: terminal_key,
        mods,
        consumed_mods,
        composing: false,
        unshifted_codepoint,
        text,
    })
}

/// Map one Crossterm mouse event inside the terminal inner rectangle to a
/// MOUSE command. Returns `None` for events outside the rectangle.
#[must_use]
pub fn mouse_command(
    mouse: MouseEvent,
    inner: Rect,
    operation_id: u64,
) -> Option<TerminalInputCommand> {
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    if mouse.column < inner.x
        || mouse.row < inner.y
        || mouse.column >= inner.x.saturating_add(inner.width)
        || mouse.row >= inner.y.saturating_add(inner.height)
    {
        return None;
    }
    let (action, button) = match mouse.kind {
        MouseEventKind::Down(button) => (TerminalMouseAction::Press, Some(map_button(button))),
        MouseEventKind::Up(button) => (TerminalMouseAction::Release, Some(map_button(button))),
        MouseEventKind::Drag(button) => (TerminalMouseAction::Motion, Some(map_button(button))),
        MouseEventKind::Moved => (TerminalMouseAction::Motion, None),
        MouseEventKind::ScrollUp => (
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::WheelUp),
        ),
        MouseEventKind::ScrollDown => (
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::WheelDown),
        ),
        MouseEventKind::ScrollLeft => (
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::WheelLeft),
        ),
        MouseEventKind::ScrollRight => (
            TerminalMouseAction::Press,
            Some(TerminalMouseButton::WheelRight),
        ),
    };
    Some(TerminalInputCommand::Mouse {
        operation_id,
        action,
        button,
        mods: mods_from_crossterm(mouse.modifiers),
        col: mouse.column - inner.x,
        row: mouse.row - inner.y,
        x_px: 0,
        y_px: 0,
    })
}

fn map_button(button: MouseButton) -> TerminalMouseButton {
    match button {
        MouseButton::Left => TerminalMouseButton::Left,
        MouseButton::Right => TerminalMouseButton::Right,
        MouseButton::Middle => TerminalMouseButton::Middle,
    }
}

/// FOCUS command for host focus changes.
#[must_use]
pub fn focus_command(focused: bool, operation_id: u64) -> TerminalInputCommand {
    TerminalInputCommand::Focus {
        operation_id,
        focused,
    }
}

/// RESIZE command with cell geometry and no pixel geometry.
#[must_use]
pub fn resize_command(rows: u16, cols: u16, operation_id: u64) -> TerminalInputCommand {
    TerminalInputCommand::Resize {
        operation_id,
        rows,
        cols,
        width_px: 0,
        height_px: 0,
    }
}

/// One admitted operation waiting for its INPUT_RESULT.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InFlightOperation {
    pub operation_id: u64,
    pub paste: bool,
}

/// One operation waiting locally for a free window slot.
#[derive(Clone, Debug, PartialEq, Eq)]
struct QueuedOperation {
    operation_id: u64,
    paste: bool,
    frames: Vec<Vec<u8>>,
    bytes: usize,
}

/// Why the client refused to queue an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputWindowError {
    /// The local queue already holds `MAX_QUEUED_INPUT_BYTES`.
    QueueFull { queued_bytes: usize },
    /// Operation ids for this route generation are exhausted.
    IdsExhausted,
}

impl std::fmt::Display for InputWindowError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QueueFull { queued_bytes } => write!(
                formatter,
                "terminal input unavailable: {queued_bytes} bytes queued at the {MAX_QUEUED_INPUT_BYTES} byte client bound"
            ),
            Self::IdsExhausted => write!(
                formatter,
                "terminal input unavailable: operation ids exhausted"
            ),
        }
    }
}

/// Client-side per-route input window.
///
/// Operation ids start at 1 for each attach and increase strictly. At most
/// `MAX_IN_FLIGHT_OPERATIONS` operations wait for a result; further
/// operations queue locally in order up to `MAX_QUEUED_INPUT_BYTES`.
#[derive(Debug, Default)]
pub struct InputWindow {
    next_operation_id: u64,
    in_flight: VecDeque<InFlightOperation>,
    queued: VecDeque<QueuedOperation>,
    queued_bytes: usize,
}

impl InputWindow {
    /// Fresh window for a new attach.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_operation_id: 1,
            in_flight: VecDeque::new(),
            queued: VecDeque::new(),
            queued_bytes: 0,
        }
    }

    /// Reserve the next operation id.
    pub fn next_operation_id(&mut self) -> Result<u64, InputWindowError> {
        let id = self.next_operation_id;
        if id == 0 {
            return Err(InputWindowError::IdsExhausted);
        }
        self.next_operation_id = id.checked_add(1).unwrap_or(0);
        Ok(id)
    }

    /// Admit one operation. Returns the frames to write now, in order, which
    /// may include earlier queued operations that a free slot released.
    pub fn admit(
        &mut self,
        operation_id: u64,
        paste: bool,
        frames: Vec<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, InputWindowError> {
        let bytes = frames.iter().map(Vec::len).sum::<usize>();
        if !self.queued.is_empty() || self.in_flight.len() >= MAX_IN_FLIGHT_OPERATIONS {
            if self.queued_bytes.saturating_add(bytes) > MAX_QUEUED_INPUT_BYTES {
                return Err(InputWindowError::QueueFull {
                    queued_bytes: self.queued_bytes,
                });
            }
            self.queued_bytes += bytes;
            self.queued.push_back(QueuedOperation {
                operation_id,
                paste,
                frames,
                bytes,
            });
            return Ok(self.release());
        }
        self.in_flight.push_back(InFlightOperation {
            operation_id,
            paste,
        });
        Ok(frames)
    }

    /// Complete one operation by id. Returns the completed entry and the frames
    /// released from the local queue, in order.
    pub fn complete(&mut self, operation_id: u64) -> (Option<InFlightOperation>, Vec<Vec<u8>>) {
        let position = self
            .in_flight
            .iter()
            .position(|entry| entry.operation_id == operation_id);
        let completed = position.and_then(|index| self.in_flight.remove(index));
        (completed, self.release())
    }

    fn release(&mut self) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        while self.in_flight.len() < MAX_IN_FLIGHT_OPERATIONS {
            let Some(next) = self.queued.pop_front() else {
                break;
            };
            self.queued_bytes = self.queued_bytes.saturating_sub(next.bytes);
            self.in_flight.push_back(InFlightOperation {
                operation_id: next.operation_id,
                paste: next.paste,
            });
            frames.extend(next.frames);
        }
        frames
    }

    /// Operations waiting for a result.
    #[must_use]
    pub fn in_flight_len(&self) -> usize {
        self.in_flight.len()
    }

    /// Whether a paste operation is in flight or queued.
    #[must_use]
    pub fn has_paste(&self) -> bool {
        self.in_flight.iter().any(|entry| entry.paste)
            || self.queued.iter().any(|entry| entry.paste)
    }

    /// Bytes waiting in the local queue.
    #[must_use]
    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn uppercase_letter_consumes_shift_and_carries_text() {
        match key_command(key(KeyCode::Char('A'), KeyModifiers::SHIFT), 7) {
            Some(TerminalInputCommand::Key {
                operation_id,
                action,
                key,
                mods,
                consumed_mods,
                composing,
                unshifted_codepoint,
                text,
            }) => {
                assert_eq!(operation_id, 7);
                assert_eq!(action, TerminalKeyAction::Press);
                assert_eq!(key, TerminalKey::KeyA);
                assert_eq!(mods & terminal_mods::SHIFT, terminal_mods::SHIFT);
                assert_eq!(consumed_mods, terminal_mods::SHIFT);
                assert!(!composing);
                assert_eq!(unshifted_codepoint, u32::from('a'));
                assert_eq!(text, "A");
            }
            other => panic!("expected a KEY command, got {other:?}"),
        }
    }

    #[test]
    fn control_chord_sends_no_text() {
        match key_command(key(KeyCode::Char('c'), KeyModifiers::CONTROL), 1) {
            Some(TerminalInputCommand::Key {
                key, mods, text, ..
            }) => {
                assert_eq!(key, TerminalKey::KeyC);
                assert_eq!(mods, terminal_mods::CTRL);
                assert!(text.is_empty());
            }
            other => panic!("expected a KEY command, got {other:?}"),
        }
    }

    #[test]
    fn back_tab_is_shift_tab_and_null_is_not_sent() {
        match key_command(key(KeyCode::BackTab, KeyModifiers::NONE), 1) {
            Some(TerminalInputCommand::Key { key, mods, .. }) => {
                assert_eq!(key, TerminalKey::Tab);
                assert_eq!(mods, terminal_mods::SHIFT);
            }
            other => panic!("expected a KEY command, got {other:?}"),
        }
        assert!(key_command(key(KeyCode::Null, KeyModifiers::NONE), 1).is_none());
    }

    #[test]
    fn shifted_symbol_maps_to_its_physical_key() {
        match key_command(key(KeyCode::Char('!'), KeyModifiers::NONE), 1) {
            Some(TerminalInputCommand::Key {
                key,
                consumed_mods,
                unshifted_codepoint,
                text,
                ..
            }) => {
                assert_eq!(key, TerminalKey::Digit1);
                assert_eq!(consumed_mods, terminal_mods::SHIFT);
                assert_eq!(unshifted_codepoint, u32::from('1'));
                assert_eq!(text, "!");
            }
            other => panic!("expected a KEY command, got {other:?}"),
        }
    }

    #[test]
    fn mouse_inside_the_rect_is_relative_and_wheel_is_a_press() {
        let inner = Rect::new(5, 3, 10, 4);
        let down = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 7,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };
        match mouse_command(down, inner, 3) {
            Some(TerminalInputCommand::Mouse {
                operation_id,
                action,
                button,
                col,
                row,
                x_px,
                y_px,
                ..
            }) => {
                assert_eq!(operation_id, 3);
                assert_eq!(action, TerminalMouseAction::Press);
                assert_eq!(button, Some(TerminalMouseButton::Left));
                assert_eq!((col, row), (2, 1));
                assert_eq!((x_px, y_px), (0, 0));
            }
            other => panic!("expected a MOUSE command, got {other:?}"),
        }
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        match mouse_command(wheel, inner, 4) {
            Some(TerminalInputCommand::Mouse { action, button, .. }) => {
                assert_eq!(action, TerminalMouseAction::Press);
                assert_eq!(button, Some(TerminalMouseButton::WheelDown));
            }
            other => panic!("expected a MOUSE command, got {other:?}"),
        }
        let outside = MouseEvent {
            kind: MouseEventKind::Moved,
            column: 15,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        assert!(mouse_command(outside, inner, 5).is_none());
    }

    #[test]
    fn kit_mouse_bits_follow_dec_tracking_modes() {
        assert_eq!(kit_mouse_bits(0), 0);
        assert_eq!(
            kit_mouse_bits(mode_bits::MOUSE_NORMAL | mode_bits::MOUSE_SGR),
            0b1001
        );
        assert_eq!(kit_mouse_bits(mode_bits::MOUSE_ANY), 0b0010);
        assert_eq!(kit_mouse_bits(mode_bits::MOUSE_BUTTON), 0b0100);
        assert!(!mouse_tracking_enabled(mode_bits::MOUSE_SGR));
        assert!(mouse_tracking_enabled(mode_bits::MOUSE_BUTTON));
    }

    #[test]
    fn window_queues_beyond_the_in_flight_bound_and_releases_in_order() {
        let mut window = InputWindow::new();
        for index in 0..MAX_IN_FLIGHT_OPERATIONS {
            let id = window.next_operation_id().expect("id");
            assert_eq!(id, index as u64 + 1);
            let released = window
                .admit(id, false, vec![vec![index as u8]])
                .expect("admitted");
            assert_eq!(released.len(), 1);
        }
        assert_eq!(window.in_flight_len(), MAX_IN_FLIGHT_OPERATIONS);
        let queued_id = window.next_operation_id().expect("id");
        let released = window
            .admit(queued_id, true, vec![vec![0xAA], vec![0xBB]])
            .expect("queued");
        assert!(released.is_empty());
        assert_eq!(window.queued_bytes(), 2);
        assert!(window.has_paste());
        let (completed, released) = window.complete(1);
        assert_eq!(completed.map(|entry| entry.operation_id), Some(1));
        assert_eq!(released, vec![vec![0xAA], vec![0xBB]]);
        assert_eq!(window.in_flight_len(), MAX_IN_FLIGHT_OPERATIONS);
        assert_eq!(window.queued_bytes(), 0);
        let (missing, released) = window.complete(999);
        assert!(missing.is_none());
        assert!(released.is_empty());
    }

    #[test]
    fn window_rejects_when_the_local_queue_is_full() {
        let mut window = InputWindow::new();
        for _ in 0..MAX_IN_FLIGHT_OPERATIONS {
            let id = window.next_operation_id().expect("id");
            window.admit(id, false, vec![vec![1]]).expect("admitted");
        }
        let id = window.next_operation_id().expect("id");
        assert!(matches!(
            window.admit(id, false, vec![vec![0; MAX_QUEUED_INPUT_BYTES + 1]]),
            Err(InputWindowError::QueueFull { queued_bytes: 0 })
        ));
    }
}
