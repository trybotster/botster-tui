use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNode {
    pub id: &'static str,
    pub kind: UiNodeKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiNodeKind {
    Stack {
        direction: StackDirection,
        children: Vec<UiNode>,
    },
    Panel {
        title: &'static str,
        child: Box<UiNode>,
    },
    List {
        items: Vec<ListRow>,
        selected: usize,
    },
    Form {
        fields: Vec<FormField>,
    },
    Dialog {
        title: &'static str,
        body: &'static str,
        action: ActionBinding,
    },
    TerminalView {
        session_id: &'static str,
        title: &'static str,
        lines: Vec<&'static str>,
    },
    Text(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StackDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListRow {
    pub id: &'static str,
    pub label: &'static str,
    pub action: ActionBinding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormField {
    pub id: &'static str,
    pub label: &'static str,
    pub value: &'static str,
    pub action: ActionBinding,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionBinding {
    pub id: SemanticActionId,
    pub target_id: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticActionId {
    SessionSelect,
    SessionRename,
    DialogDismiss,
    PresentationSet,
    TerminalFocus,
    TerminalInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiActionRequest {
    Semantic {
        binding: ActionBinding,
    },
    LocalPresentation {
        key: &'static str,
        operation: PresentationOperation,
    },
    TerminalForward {
        session_id: &'static str,
        bytes: Vec<u8>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentationOperation {
    Set,
    Clear,
    Toggle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HitRegion {
    pub node_id: &'static str,
    pub role: HitRole,
    pub rect: Rect,
    pub binding: Option<ActionBinding>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HitRole {
    Panel,
    ListRow,
    FormField,
    Dialog,
    TerminalView,
    Text,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HitMap {
    regions: Vec<HitRegion>,
}

impl HitMap {
    pub fn push(&mut self, region: HitRegion) {
        self.regions.push(region);
    }

    pub fn lookup(&self, column: u16, row: u16) -> Option<&HitRegion> {
        self.regions
            .iter()
            .rev()
            .find(|region| contains(region.rect, column, row))
    }

    pub fn regions(&self) -> &[HitRegion] {
        &self.regions
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputDispatch {
    Hover { node_id: &'static str },
    Scroll { node_id: &'static str, lines: i16 },
    Action(UiActionRequest),
    Ignored,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoundationProof {
    pub foundation: &'static str,
    pub frame_width: u16,
    pub frame_height: u16,
    pub hit_regions: usize,
    pub semantic_actions: usize,
    pub terminal_forwarded: bool,
    pub redraws: usize,
}

#[derive(Clone, Debug)]
pub struct RedrawBudget {
    frame_interval: Duration,
    last_draw: Option<Instant>,
    dirty_nodes: BTreeSet<&'static str>,
    draws: usize,
}

impl RedrawBudget {
    pub fn new(frame_interval: Duration) -> Self {
        Self {
            frame_interval,
            last_draw: None,
            dirty_nodes: BTreeSet::new(),
            draws: 0,
        }
    }

    pub fn mark_output(&mut self, node_id: &'static str) {
        self.dirty_nodes.insert(node_id);
    }

    pub fn maybe_draw(&mut self, now: Instant) -> Option<BTreeSet<&'static str>> {
        if self.dirty_nodes.is_empty() {
            return None;
        }

        let ready = self
            .last_draw
            .is_none_or(|last_draw| now.duration_since(last_draw) >= self.frame_interval);

        if !ready {
            return None;
        }

        self.last_draw = Some(now);
        self.draws += 1;
        Some(std::mem::take(&mut self.dirty_nodes))
    }

    pub fn draws(&self) -> usize {
        self.draws
    }
}

pub fn render_fixture(root: &UiNode, width: u16, height: u16) -> (Vec<String>, HitMap) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    let mut hit_map = HitMap::default();

    terminal
        .draw(|frame| render_node(frame, frame.area(), root, &mut hit_map))
        .expect("test backend should draw fixture");

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

pub fn dispatch_event(event: Event, hit_map: &HitMap) -> InputDispatch {
    match event {
        Event::Mouse(mouse) => dispatch_mouse(mouse, hit_map),
        Event::Key(key) => dispatch_key(key, hit_map),
        _ => InputDispatch::Ignored,
    }
}

pub fn run_foundation_proof() -> FoundationProof {
    let root = fixture_tree();
    let (_lines, hit_map) = render_fixture(&root, 80, 24);

    let click = dispatch_event(
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 1,
            modifiers: KeyModifiers::empty(),
        }),
        &hit_map,
    );
    let terminal = dispatch_event(
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 42,
            row: 11,
            modifiers: KeyModifiers::empty(),
        }),
        &hit_map,
    );

    let start = Instant::now();
    let mut budget = RedrawBudget::new(Duration::from_millis(16));
    for index in 0..100 {
        budget.mark_output("terminal-main");
        let _ = budget.maybe_draw(start + Duration::from_millis(index));
    }

    FoundationProof {
        foundation: "ratatui+crossterm",
        frame_width: 80,
        frame_height: 24,
        hit_regions: hit_map.regions().len(),
        semantic_actions: usize::from(matches!(click, InputDispatch::Action(_))),
        terminal_forwarded: matches!(
            terminal,
            InputDispatch::Action(UiActionRequest::Semantic {
                binding: ActionBinding {
                    id: SemanticActionId::TerminalFocus,
                    ..
                }
            })
        ),
        redraws: budget.draws(),
    }
}

pub fn fixture_tree() -> UiNode {
    UiNode {
        id: "root-stack",
        kind: UiNodeKind::Stack {
            direction: StackDirection::Horizontal,
            children: vec![
                UiNode {
                    id: "workspace-panel",
                    kind: UiNodeKind::Panel {
                        title: "Workspace",
                        child: Box::new(UiNode {
                            id: "session-list",
                            kind: UiNodeKind::List {
                                selected: 0,
                                items: vec![
                                    ListRow {
                                        id: "session-alpha",
                                        label: "alpha",
                                        action: ActionBinding {
                                            id: SemanticActionId::SessionSelect,
                                            target_id: "session-alpha",
                                        },
                                    },
                                    ListRow {
                                        id: "session-beta",
                                        label: "beta",
                                        action: ActionBinding {
                                            id: SemanticActionId::SessionSelect,
                                            target_id: "session-beta",
                                        },
                                    },
                                ],
                            },
                        }),
                    },
                },
                UiNode {
                    id: "detail-stack",
                    kind: UiNodeKind::Stack {
                        direction: StackDirection::Vertical,
                        children: vec![
                            UiNode {
                                id: "rename-form",
                                kind: UiNodeKind::Form {
                                    fields: vec![FormField {
                                        id: "session-name",
                                        label: "Name",
                                        value: "alpha",
                                        action: ActionBinding {
                                            id: SemanticActionId::SessionRename,
                                            target_id: "session-alpha",
                                        },
                                    }],
                                },
                            },
                            UiNode {
                                id: "terminal-main",
                                kind: UiNodeKind::TerminalView {
                                    session_id: "session-alpha",
                                    title: "Terminal",
                                    lines: vec!["$ cargo test", "running 7 tests"],
                                },
                            },
                            UiNode {
                                id: "dismiss-dialog",
                                kind: UiNodeKind::Dialog {
                                    title: "Confirm",
                                    body: "Dismiss modal?",
                                    action: ActionBinding {
                                        id: SemanticActionId::DialogDismiss,
                                        target_id: "dismiss-dialog",
                                    },
                                },
                            },
                        ],
                    },
                },
            ],
        },
    }
}

fn render_node(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    match &node.kind {
        UiNodeKind::Stack {
            direction,
            children,
        } => {
            let direction = match direction {
                StackDirection::Horizontal => Direction::Horizontal,
                StackDirection::Vertical => Direction::Vertical,
            };
            let constraints = even_constraints(children.len());
            let chunks = Layout::default()
                .direction(direction)
                .constraints(constraints)
                .split(area);
            for (child, chunk) in children.iter().zip(chunks.iter()) {
                render_node(frame, *chunk, child, hit_map);
            }
        }
        UiNodeKind::Panel { title, child } => {
            hit_map.push(HitRegion {
                node_id: node.id,
                role: HitRole::Panel,
                rect: area,
                binding: None,
            });
            let block = Block::default().title(*title).borders(Borders::ALL);
            let inner = block.inner(area);
            frame.render_widget(block, area);
            render_node(frame, inner, child, hit_map);
        }
        UiNodeKind::List { items, selected } => {
            let list_items = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    let marker = if index == *selected { "> " } else { "  " };
                    ListItem::new(format!("{marker}{}", item.label))
                })
                .collect::<Vec<_>>();
            frame.render_widget(List::new(list_items), area);
            for (index, item) in items.iter().enumerate() {
                let row = area.y + u16::try_from(index).unwrap_or(u16::MAX);
                if row < area.y.saturating_add(area.height) {
                    hit_map.push(HitRegion {
                        node_id: item.id,
                        role: HitRole::ListRow,
                        rect: Rect::new(area.x, row, area.width, 1),
                        binding: Some(item.action.clone()),
                    });
                }
            }
        }
        UiNodeKind::Form { fields } => {
            let rows = fields
                .iter()
                .map(|field| format!("{}: {}", field.label, field.value))
                .collect::<Vec<_>>()
                .join("\n");
            frame.render_widget(Paragraph::new(rows), area);
            for (index, field) in fields.iter().enumerate() {
                let row = area.y + u16::try_from(index).unwrap_or(u16::MAX);
                if row < area.y.saturating_add(area.height) {
                    hit_map.push(HitRegion {
                        node_id: field.id,
                        role: HitRole::FormField,
                        rect: Rect::new(area.x, row, area.width, 1),
                        binding: Some(field.action.clone()),
                    });
                }
            }
        }
        UiNodeKind::Dialog {
            title,
            body,
            action,
        } => {
            hit_map.push(HitRegion {
                node_id: node.id,
                role: HitRole::Dialog,
                rect: area,
                binding: Some(action.clone()),
            });
            frame.render_widget(
                Paragraph::new(*body)
                    .block(Block::default().title(*title).borders(Borders::ALL))
                    .wrap(Wrap { trim: true }),
                area,
            );
        }
        UiNodeKind::TerminalView {
            session_id: _,
            title,
            lines,
        } => {
            hit_map.push(HitRegion {
                node_id: node.id,
                role: HitRole::TerminalView,
                rect: area,
                binding: Some(ActionBinding {
                    id: SemanticActionId::TerminalFocus,
                    target_id: node.id,
                }),
            });
            frame.render_widget(
                Paragraph::new(lines.join("\n"))
                    .block(
                        Block::default()
                            .title(*title)
                            .borders(Borders::ALL)
                            .border_style(Style::default().add_modifier(Modifier::BOLD)),
                    )
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
        UiNodeKind::Text(text) => {
            hit_map.push(HitRegion {
                node_id: node.id,
                role: HitRole::Text,
                rect: area,
                binding: None,
            });
            frame.render_widget(Paragraph::new(*text), area);
        }
    }
}

fn dispatch_mouse(mouse: MouseEvent, hit_map: &HitMap) -> InputDispatch {
    let Some(region) = hit_map.lookup(mouse.column, mouse.row) else {
        return InputDispatch::Ignored;
    };

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(_) => InputDispatch::Hover {
            node_id: region.node_id,
        },
        MouseEventKind::ScrollDown => InputDispatch::Scroll {
            node_id: region.node_id,
            lines: 3,
        },
        MouseEventKind::ScrollUp => InputDispatch::Scroll {
            node_id: region.node_id,
            lines: -3,
        },
        MouseEventKind::Down(MouseButton::Left) => match region.role {
            HitRole::TerminalView => InputDispatch::Action(UiActionRequest::Semantic {
                binding: ActionBinding {
                    id: SemanticActionId::TerminalFocus,
                    target_id: region.node_id,
                },
            }),
            HitRole::Dialog => InputDispatch::Action(UiActionRequest::LocalPresentation {
                key: "dismiss-dialog.open",
                operation: PresentationOperation::Clear,
            }),
            _ => region
                .binding
                .clone()
                .map(|binding| InputDispatch::Action(UiActionRequest::Semantic { binding }))
                .unwrap_or(InputDispatch::Ignored),
        },
        _ => InputDispatch::Ignored,
    }
}

fn dispatch_key(key: KeyEvent, hit_map: &HitMap) -> InputDispatch {
    let terminal = hit_map
        .regions()
        .iter()
        .find(|region| region.role == HitRole::TerminalView);

    let Some(terminal) = terminal else {
        return InputDispatch::Ignored;
    };

    let bytes = match key.code {
        KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            vec![(character as u8) & 0x1f]
        }
        KeyCode::Char(character) => character.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\n'],
        KeyCode::Tab => vec![b'\t'],
        _ => return InputDispatch::Ignored,
    };

    InputDispatch::Action(UiActionRequest::TerminalForward {
        session_id: terminal.node_id,
        bytes,
    })
}

fn even_constraints(count: usize) -> Vec<Constraint> {
    if count == 0 {
        return Vec::new();
    }

    let percent = 100 / u16::try_from(count).unwrap_or(1).max(1);
    let mut constraints = vec![Constraint::Percentage(percent); count];
    if let Some(last) = constraints.last_mut() {
        *last = Constraint::Min(1);
    }
    constraints
}

fn contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

pub fn adapter_mapping() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("stack(horizontal)", "HSplit"),
        ("stack(vertical)", "VSplit"),
        ("panel", "BlockConfig"),
        ("list", "WidgetType::List"),
        ("text_input", "WidgetType::Input"),
        ("terminal_view", "WidgetType::Terminal"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_split_panes_list_form_dialog_and_terminal_view() {
        let (lines, hit_map) = render_fixture(&fixture_tree(), 80, 24);

        let frame = lines.join("\n");
        assert!(frame.contains("Workspace"));
        assert!(frame.contains("> alpha"));
        assert!(frame.contains("Name: alpha"));
        assert!(frame.contains("Terminal"));
        assert!(frame.contains("Dismiss modal?"));

        let roles = hit_map
            .regions()
            .iter()
            .map(|region| region.role)
            .collect::<BTreeSet<_>>();

        assert!(roles.contains(&HitRole::Panel));
        assert!(roles.contains(&HitRole::ListRow));
        assert!(roles.contains(&HitRole::FormField));
        assert!(roles.contains(&HitRole::Dialog));
        assert!(roles.contains(&HitRole::TerminalView));
    }

    #[test]
    fn hit_map_uses_stable_node_ids_not_screen_coordinates() {
        let (_lines, hit_map) = render_fixture(&fixture_tree(), 80, 24);

        assert_eq!(
            hit_map.lookup(3, 1).map(|hit| hit.node_id),
            Some("session-alpha")
        );
        assert_eq!(
            hit_map.lookup(42, 11).map(|hit| hit.node_id),
            Some("terminal-main")
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .all(|region| !region.node_id.is_empty())
        );
    }

    #[test]
    fn crossterm_mouse_events_translate_to_hover_click_and_scroll() {
        let (_lines, hit_map) = render_fixture(&fixture_tree(), 80, 24);

        assert_eq!(
            dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 3,
                    row: 1,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Hover {
                node_id: "session-alpha"
            }
        );

        assert_eq!(
            dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: 3,
                    row: 2,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Scroll {
                node_id: "session-beta",
                lines: 3,
            }
        );

        assert_eq!(
            dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 3,
                    row: 1,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Action(UiActionRequest::Semantic {
                binding: ActionBinding {
                    id: SemanticActionId::SessionSelect,
                    target_id: "session-alpha",
                }
            })
        );
    }

    #[test]
    fn dialog_click_maps_to_client_local_presentation_state() {
        let (_lines, hit_map) = render_fixture(&fixture_tree(), 80, 24);

        assert_eq!(
            dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 42,
                    row: 20,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Action(UiActionRequest::LocalPresentation {
                key: "dismiss-dialog.open",
                operation: PresentationOperation::Clear,
            })
        );
    }

    #[test]
    fn terminal_view_focus_and_input_forwarding_are_separate_from_widget_actions() {
        let (_lines, hit_map) = render_fixture(&fixture_tree(), 80, 24);

        assert_eq!(
            dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 42,
                    row: 11,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Action(UiActionRequest::Semantic {
                binding: ActionBinding {
                    id: SemanticActionId::TerminalFocus,
                    target_id: "terminal-main",
                }
            })
        );

        assert_eq!(
            dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
                &hit_map,
            ),
            InputDispatch::Action(UiActionRequest::TerminalForward {
                session_id: "terminal-main",
                bytes: vec![10],
            })
        );
    }

    #[test]
    fn frequent_terminal_output_coalesces_into_bounded_redraws() {
        let start = Instant::now();
        let mut budget = RedrawBudget::new(Duration::from_millis(16));

        for index in 0..100 {
            budget.mark_output("terminal-main");
            let _ = budget.maybe_draw(start + Duration::from_millis(index));
        }

        assert_eq!(budget.draws(), 7);
    }

    #[test]
    fn adapter_mapping_targets_existing_rust_render_tree_shapes() {
        let mapping = adapter_mapping();

        assert_eq!(mapping["stack(horizontal)"], "HSplit");
        assert_eq!(mapping["stack(vertical)"], "VSplit");
        assert_eq!(mapping["panel"], "BlockConfig");
        assert_eq!(mapping["list"], "WidgetType::List");
        assert_eq!(mapping["text_input"], "WidgetType::Input");
        assert_eq!(mapping["terminal_view"], "WidgetType::Terminal");
    }
}
