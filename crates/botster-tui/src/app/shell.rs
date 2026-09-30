use super::*;

pub fn run(args: AppArgs) -> io::Result<()> {
    let hub_io = HubIo::with_terminal_input()?;
    let mut terminal = setup_terminal()?;
    let run_result = run_loop(&mut terminal, args, hub_io);
    let restore_result = restore_terminal(&mut terminal);

    match (run_result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

pub(super) fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    if let Err(error) = execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        EnableFocusChange
    ) {
        let _ = disable_raw_mode();
        return Err(error);
    }

    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let mut stdout = io::stdout();
            let _ = execute!(
                stdout,
                DisableFocusChange,
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen,
                Show
            );
            let _ = disable_raw_mode();
            Err(error)
        }
    }
}

pub(super) fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
) -> io::Result<()> {
    let leave_result = execute!(
        terminal.backend_mut(),
        DisableFocusChange,
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
    let raw_result = disable_raw_mode();
    let cursor_result = terminal.show_cursor();

    leave_result?;
    raw_result?;
    cursor_result
}

/// The interactive event loop.
///
/// One wait per turn: `HubIo::next_wake` blocks until an input event, a Hub
/// frame, a request completion, or the earliest absolute deadline. Every wake
/// that is already available is applied before one paint. There is no
/// periodic poll.
pub(super) fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    args: AppArgs,
    hub_io: HubIo,
) -> io::Result<()> {
    let mut app = TuiApp::new_with_runtime_context(
        args.daemon_endpoint(),
        args.connection_error,
        args.hub_data_dir.is_some(),
        hub_io,
    );
    app.connect();
    let mut router = InputRouter::new(renderer::action_request_context());
    let mut routed_surface_id = None;
    let mut running = true;
    while running {
        let active_surface_id = app.active_plugin_surface_id().map(ToOwned::to_owned);
        if active_surface_id != routed_surface_id {
            router = InputRouter::new(match active_surface_id.as_deref() {
                Some(surface_id) => renderer::action_request_context_for(surface_id),
                None => renderer::action_request_context(),
            });
            routed_surface_id = active_surface_id;
        }
        app.set_drafts(router.draft_values());

        let render_state = router.render_state();
        let mut hit_map = HitMap::default();
        app.prepare_paint();
        terminal.draw(|frame| draw(frame, &mut hit_map, &app, &render_state))?;
        app.apply_terminal_mouse_mode(&mut hit_map);
        app.sync_terminal_pane_size(&hit_map);
        router.reconcile(&hit_map);

        let wake = app.next_wake();
        running = apply_wake(&mut app, &mut router, &hit_map, wake);
        let mut applied = 1;
        while running && applied < WAKE_BATCH {
            let Some(wake) = app.try_next_wake() else {
                break;
            };
            running = apply_wake(&mut app, &mut router, &hit_map, wake);
            applied += 1;
        }
    }
    if !app.shutdown() {
        return Err(io::Error::other(
            "the Hub link or input thread did not stop within the shutdown bound",
        ));
    }
    Ok(())
}

/// Apply one wake. Returns false when the application should exit.
pub(super) fn apply_wake(
    app: &mut TuiApp,
    router: &mut InputRouter,
    hit_map: &HitMap,
    wake: AppWake,
) -> bool {
    match wake {
        AppWake::Input(event) => route_input_event(app, router, hit_map, event),
        AppWake::Shutdown => false,
        other => {
            app.apply_wake(other);
            true
        }
    }
}

/// Chords the TUI keeps for itself. They never reach a session, whatever has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostKey {
    /// Ctrl+P: move focus out of the terminal to the workspace toolbar.
    Menu,
    /// Ctrl+J: select the next session row.
    NextSession,
    /// Ctrl+K: select the previous session row.
    PreviousSession,
    /// Shift+PageUp / PageDown / Home / End: scroll the terminal projection.
    Scroll(HostScroll),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HostScroll {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// Matches every key kind, so a reserved chord's release never reaches a
/// session either. Release reporting needs keyboard enhancement flags, which
/// the TUI does not enable today.
pub(super) fn host_key(key: KeyEvent) -> Option<HostKey> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(HostKey::Menu),
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => Some(HostKey::NextSession),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(HostKey::PreviousSession),
        (KeyCode::PageUp, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::PageUp)),
        (KeyCode::PageDown, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::PageDown)),
        (KeyCode::Home, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::Top)),
        (KeyCode::End, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::Bottom)),
        _ => None,
    }
}

pub(super) fn apply_host_key(
    app: &mut TuiApp,
    router: &mut InputRouter,
    hit_map: &HitMap,
    host_key: HostKey,
) {
    match host_key {
        HostKey::Menu => {
            let focused = matches!(
                router.focus_node(WORKSPACE_MENU_NODE, hit_map),
                InputDispatch::Focus { .. }
            );
            // The toolbar can overflow at narrow widths; still leave the terminal.
            if !focused && is_terminal_node(router.focused_node_id()) {
                router.focus_next(hit_map);
            }
        }
        HostKey::NextSession | HostKey::PreviousSession => {
            let count = app.sessions.len();
            if count == 0 {
                return;
            }
            let current = app.selected_session.as_deref().and_then(|id| {
                app.sessions
                    .iter()
                    .position(|session| session.session_id == id)
            });
            let next = match (host_key, current) {
                (HostKey::NextSession, Some(index)) => (index + 1) % count,
                (HostKey::NextSession, None) => 0,
                (_, Some(index)) => (index + count - 1) % count,
                (_, None) => count - 1,
            };
            let session_id = app.sessions[next].session_id.clone();
            // Focus the row so Enter attaches it. A modal hides the list, and
            // then the chord changes nothing.
            if matches!(
                router.focus_node(&format!("tui-session-{session_id}"), hit_map),
                InputDispatch::Focus { .. }
            ) {
                app.set_selected_session(Some(session_id));
            }
        }
        HostKey::Scroll(scroll) => {
            let page = i32::from(app.terminal_viewport_size.rows);
            app.scroll_projection(match scroll {
                HostScroll::PageUp => ScrollOp::Delta(-page),
                HostScroll::PageDown => ScrollOp::Delta(page),
                HostScroll::Top => ScrollOp::Top,
                HostScroll::Bottom => ScrollOp::Bottom,
            });
        }
    }
}

pub(super) fn route_input_event(
    app: &mut TuiApp,
    router: &mut InputRouter,
    hit_map: &HitMap,
    event: Event,
) -> bool {
    match event {
        Event::Key(key) if let Some(host_key) = host_key(key) => {
            if key.kind != KeyEventKind::Release {
                apply_host_key(app, router, hit_map, host_key);
            }
        }
        Event::Key(key) if key.kind == KeyEventKind::Press && app.handle_tui_owned_key(key) => {}
        Event::Key(key) if app.handle_focused_terminal_key(key, router.focused_node_id()) => {}
        Event::Paste(ref text)
            if app.handle_focused_terminal_paste(text, router.focused_node_id()) => {}
        Event::Mouse(mouse)
            if app.handle_focused_terminal_mouse(mouse, router.focused_node_id(), hit_map) => {}
        // The next draw measures the new pane; `sync_terminal_pane_size`
        // sends it. The router would report the previous frame's pane.
        Event::Resize(..) => {}
        Event::FocusGained => app.handle_host_focus(true),
        Event::FocusLost => app.handle_host_focus(false),
        Event::Key(key) if key.kind == KeyEventKind::Press && should_quit(key) => return false,
        event => {
            let dispatch = router.dispatch_event(event, hit_map);
            app.sync_focused_session(router.selected_row_value("tui-session-list"));
            app.handle_dispatch(dispatch);
        }
    }
    true
}

pub(super) fn draw(
    frame: &mut Frame<'_>,
    hit_map: &mut HitMap,
    app: &TuiApp,
    render_state: &RenderState,
) {
    if app.uses_workspace_shell() {
        draw_workspace_shell(frame, hit_map, app, render_state);
        return;
    }
    let node = app.surface();
    renderer::render_node_with_presentation_state(
        frame,
        frame.area(),
        &node,
        hit_map,
        render_state,
        &app.plugin_presentation,
    );
    app.paint_ghostty_projection(frame, hit_map);
}

pub(super) fn draw_workspace_shell(
    frame: &mut Frame<'_>,
    hit_map: &mut HitMap,
    app: &TuiApp,
    render_state: &RenderState,
) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    // This is deliberately a multi-root render into one HitMap. Confirmation
    // dialogs and plugin surfaces must remain excluded by uses_workspace_shell:
    // a modal root clears regions registered by earlier roots.
    let width_class = renderer::viewport_for_area(area).width_class;
    let status = app.status_summary_node(width_class);
    let alert = app.connection_alert();
    let notice = app.transient_notice_band();
    let recovery = app.recovery_notice_line();
    let quarantine = app.quarantine_band();
    let toolbar = app.workspace_toolbar();
    let navigator = app.session_navigator();
    let focused_session = app.focused_session_panel();
    for node in [
        Some(&status),
        alert.as_ref(),
        notice.as_ref(),
        recovery.as_ref(),
        quarantine.as_ref(),
        Some(&toolbar),
        Some(&navigator),
        Some(&focused_session),
    ]
    .into_iter()
    .flatten()
    {
        node.validate()
            .expect("workspace shell node should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(node)
            .expect("workspace shell node should fit TUI renderer capabilities");
    }

    let status_area = Rect::new(area.x, area.y, area.width, 1);
    renderer::render_node_with_presentation_state(
        frame,
        status_area,
        &status,
        hit_map,
        render_state,
        &app.plugin_presentation,
    );

    let mut next_y = area.y.saturating_add(1);
    for band in [
        alert.as_ref(),
        notice.as_ref(),
        recovery.as_ref(),
        quarantine.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
        let band_area = Rect::new(area.x, next_y, area.width, 1);
        renderer::render_node_with_presentation_state(
            frame,
            band_area,
            band,
            hit_map,
            render_state,
            &app.plugin_presentation,
        );
        next_y = next_y.saturating_add(1);
        if next_y >= area.y.saturating_add(area.height) {
            return;
        }
    }

    if next_y >= area.y.saturating_add(area.height) {
        return;
    }
    let toolbar_y = next_y;
    let toolbar_area = Rect::new(
        area.x,
        toolbar_y,
        area.width,
        area.y.saturating_add(area.height).saturating_sub(toolbar_y),
    );
    let overflow_open = render_state.is_expanded(WORKSPACE_TOOLBAR_OVERFLOW_ID);
    if !overflow_open {
        renderer::render_node_with_presentation_state(
            frame,
            toolbar_area,
            &toolbar,
            hit_map,
            render_state,
            &app.plugin_presentation,
        );
    }

    next_y = next_y.saturating_add(1);
    let body = Rect::new(
        area.x,
        next_y,
        area.width,
        area.y.saturating_add(area.height).saturating_sub(next_y),
    );
    if body.width > 0 && body.height > 0 {
        let panes = workspace_panes(body, app.sessions.len());
        if let Some(navigator_area) = panes.first().copied() {
            renderer::render_node_with_presentation_state(
                frame,
                navigator_area,
                &navigator,
                hit_map,
                render_state,
                &app.plugin_presentation,
            );
        }
        if let Some(terminal_area) = panes.get(1).copied() {
            renderer::render_node_with_presentation_state(
                frame,
                terminal_area,
                &focused_session,
                hit_map,
                render_state,
                &app.plugin_presentation,
            );
            // TUI-owned styled paint after kit TerminalView chrome (HitMap region).
            app.paint_ghostty_projection(frame, hit_map);
        }
    }

    if overflow_open {
        // Render an open overflow last so its occluder and regions win hit
        // testing. The menu captures focus traversal while it is expanded.
        renderer::render_node_with_presentation_state(
            frame,
            toolbar_area,
            &toolbar,
            hit_map,
            render_state,
            &app.plugin_presentation,
        );
    }
}

pub(super) fn workspace_panes(area: Rect, session_count: usize) -> Vec<Rect> {
    match renderer::viewport_for_area(area).width_class {
        UiWidthClass::Expanded => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(40), Constraint::Min(1)])
            .split(area)
            .to_vec(),
        UiWidthClass::Regular => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Min(1)])
            .split(area)
            .to_vec(),
        UiWidthClass::Compact => compact_workspace_panes(area, session_count),
    }
}

pub(super) fn compact_workspace_panes(area: Rect, session_count: usize) -> Vec<Rect> {
    if area.height < 2 {
        return vec![area];
    }
    let maximum_navigator_height = (area.height / 2)
        .clamp(3, 10)
        .min(area.height.saturating_sub(1));
    let navigator_height = u16::try_from(session_count.max(2))
        .unwrap_or(maximum_navigator_height)
        .saturating_add(2)
        .min(maximum_navigator_height)
        .max(1);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(navigator_height), Constraint::Min(1)])
        .split(area)
        .to_vec()
}

#[cfg(test)]
pub(super) fn render_app_to_lines(
    app: &TuiApp,
    width: u16,
    height: u16,
    state: &RenderState,
) -> (Vec<String>, HitMap) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    let mut hit_map = HitMap::default();
    terminal
        .draw(|frame| draw(frame, &mut hit_map, app, state))
        .expect("application shell should render");
    let buffer = terminal.backend().buffer();
    let lines = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect::<String>()
        })
        .collect();
    (lines, hit_map)
}

pub(super) fn should_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || matches!(key.code, KeyCode::Char('q' | 'Q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// User-facing text for an INPUT_RESULT that is not `Written`.
pub(super) fn input_outcome_message(result: &InputResultBody) -> String {
    let written = result
        .written_pty_bytes
        .map(|bytes| format!(" after {bytes} bytes"))
        .unwrap_or_default();
    let detail = if result.detail.is_empty() {
        String::new()
    } else {
        format!(": {}", result.detail)
    };
    let operation = result.operation_id;
    match result.outcome {
        InputOutcome::Written => format!("terminal input {operation} written"),
        InputOutcome::PartialWrite => {
            format!("terminal input {operation} partially written{written}{detail}")
        }
        InputOutcome::WriteFailed => format!("terminal input {operation} write failed{detail}"),
        InputOutcome::Cancelled => format!("terminal input {operation} cancelled{written}"),
        InputOutcome::RejectedNotWritable => {
            format!("terminal input {operation} rejected: session is not writable{detail}")
        }
        InputOutcome::RejectedTooLarge => {
            format!("terminal input {operation} rejected: payload too large{detail}")
        }
        InputOutcome::RejectedUnsafePaste => {
            format!("terminal paste {operation} rejected: unsafe paste{detail}")
        }
        InputOutcome::RejectedLaneFull => {
            format!("terminal input {operation} rejected: input lane full{detail}")
        }
        InputOutcome::RejectedProtocol => {
            format!("terminal input {operation} rejected: protocol error{detail}")
        }
        InputOutcome::SessionEnded => format!("terminal input {operation} rejected: session ended"),
        InputOutcome::OutcomeUnknown => {
            format!("terminal input {operation} outcome unknown: worker link failed{detail}")
        }
    }
}

/// Reconnect delay after `failures` consecutive connection failures.
pub(super) fn reconnect_backoff_delay(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(8);
    RECONNECT_BACKOFF_INITIAL
        .saturating_mul(1_u32 << exponent)
        .min(RECONNECT_BACKOFF_CAP)
}
