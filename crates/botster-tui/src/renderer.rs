use std::collections::{BTreeMap, BTreeSet};

use botster_core::RequestId;
use botster_core::ui::{
    UiAction, UiActionId, UiActionKind, UiActionRequest, UiCapabilityFallback, UiCapabilitySet,
    UiChild, UiDialogPresentation, UiFieldSchema, UiFormValues, UiHeightClass,
    UiKeyboardCapability, UiNode, UiNodeId, UiNodeKind, UiPointer, UiSurfaceId, UiWidthClass,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use serde_json::{Map, Value, json};

pub const DEMO_SURFACE_ID: &str = "botster-tui.demo";

#[derive(Clone, Debug, PartialEq)]
pub struct HitRegion {
    pub node_id: String,
    pub role: HitRole,
    pub rect: Rect,
    pub action: Option<UiAction>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HitRole {
    Action,
    Dialog,
    Field,
    ListItem,
    Panel,
    TerminalView,
    Text,
}

#[derive(Clone, Debug, Default, PartialEq)]
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

#[derive(Clone, Debug, PartialEq)]
pub enum InputDispatch {
    Hover { node_id: String },
    Scroll { node_id: String, lines: i16 },
    Action(UiActionRequest),
    TerminalForward { node_id: String, bytes: Vec<u8> },
    Ignored,
}

#[cfg(test)]
#[derive(Clone, Debug)]
pub struct RedrawBudget {
    frame_interval: std::time::Duration,
    last_draw: Option<std::time::Instant>,
    dirty_nodes: BTreeSet<String>,
    draws: usize,
}

#[cfg(test)]
impl RedrawBudget {
    pub fn new(frame_interval: std::time::Duration) -> Self {
        Self {
            frame_interval,
            last_draw: None,
            dirty_nodes: BTreeSet::new(),
            draws: 0,
        }
    }

    pub fn mark_output(&mut self, node_id: impl Into<String>) {
        self.dirty_nodes.insert(node_id.into());
    }

    pub fn maybe_draw(&mut self, now: std::time::Instant) -> Option<BTreeSet<String>> {
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

pub fn render_node(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    root: &UiNode,
    hit_map: &mut HitMap,
) {
    render_node_inner(frame, area, root, hit_map);
}

#[cfg(test)]
pub fn render_to_lines(root: &UiNode, width: u16, height: u16) -> (Vec<String>, HitMap) {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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

pub fn tui_capabilities() -> UiCapabilitySet {
    UiCapabilitySet {
        width_classes: BTreeSet::from([
            UiWidthClass::Compact,
            UiWidthClass::Regular,
            UiWidthClass::Expanded,
        ]),
        height_classes: BTreeSet::from([UiHeightClass::Short, UiHeightClass::Regular]),
        pointer: UiPointer::Fine,
        keyboard: UiKeyboardCapability {
            text_entry: true,
            shortcuts: true,
            focus_traversal: true,
        },
        hover: false,
        clipboard: false,
        context_menu: false,
        dialog_presentations: BTreeSet::from([UiDialogPresentation::Inline]),
        table: false,
        terminal_selection: false,
        qr_code: false,
        rich_color: false,
        fallbacks: BTreeSet::from([
            UiCapabilityFallback::TableAsList,
            UiCapabilityFallback::TerminalSelectionDisabled,
            UiCapabilityFallback::ConnectionCodeText,
            UiCapabilityFallback::RichColorMuted,
            UiCapabilityFallback::DialogInline,
            UiCapabilityFallback::HoverPersistentHints,
            UiCapabilityFallback::ClipboardManual,
            UiCapabilityFallback::ContextMenuAsMenu,
        ]),
    }
}

#[cfg(test)]
pub fn primitive_registry() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("stack", "Layout"),
        ("inline", "Layout"),
        ("panel", "Block"),
        ("scroll_area", "Paragraph"),
        ("text", "Paragraph"),
        ("badge", "Paragraph"),
        ("status_dot", "Paragraph"),
        ("empty_state", "Paragraph"),
        ("list", "List"),
        ("list_item", "ListItem"),
        ("table", "Paragraph(table_as_list)"),
        ("button", "Paragraph"),
        ("dialog", "Block"),
        ("text_input", "Paragraph"),
        ("textarea", "Paragraph(read_only)"),
        ("checkbox", "Paragraph"),
        ("select", "Paragraph"),
    ])
}

pub fn demo_ui_node() -> UiNode {
    let mut root = node(
        UiNodeKind::Stack,
        "demo-root",
        json!({ "direction": "vertical" }),
    );
    root.children = vec![
        child(
            node(
                UiNodeKind::Panel,
                "demo-panel",
                json!({ "title": "botster-tui" }),
            )
            .with_children(vec![
                child(node(
                    UiNodeKind::Text,
                    "demo-title",
                    json!({ "text": "Core UiNode renderer scaffold" }),
                )),
                child(node(
                    UiNodeKind::Badge,
                    "demo-badge",
                    json!({ "label": "Ready", "tone": "success" }),
                )),
            ]),
        ),
        child(node(
            UiNodeKind::Button,
            "demo-action",
            json!({
                "label": "Select session",
                "action": { "id": "botster.session.select", "payload": { "session_id": "session-demo" } }
            }),
        )),
        child(node(
            UiNodeKind::EmptyState,
            "demo-empty",
            json!({ "title": "Hub connection not attached", "description": "Rendering a core-derived fixture." }),
        )),
    ];

    root.validate()
        .expect("demo UiNode should satisfy the core UI contract");
    tui_capabilities()
        .validate_node(&root)
        .expect("demo UiNode should fit TUI renderer capabilities");
    root
}

fn render_node_inner(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
) {
    match node.kind {
        UiNodeKind::Stack => render_stack(frame, area, node, hit_map),
        UiNodeKind::Inline => render_inline(frame, area, node, hit_map),
        UiNodeKind::Panel => render_panel(frame, area, node, hit_map),
        UiNodeKind::ScrollArea => render_children_or_text(frame, area, node, hit_map, "scroll"),
        UiNodeKind::Text => render_text(frame, area, node, hit_map),
        UiNodeKind::Badge => render_label(frame, area, node, hit_map, "badge", HitRole::Text),
        UiNodeKind::StatusDot => render_label(frame, area, node, hit_map, "status", HitRole::Text),
        UiNodeKind::EmptyState => render_empty_state(frame, area, node),
        UiNodeKind::List => render_list(frame, area, node, hit_map),
        UiNodeKind::ListItem => render_list_item(frame, area, node, hit_map),
        UiNodeKind::Table => render_table_as_list(frame, area, node),
        UiNodeKind::Button | UiNodeKind::IconButton | UiNodeKind::MenuItem => {
            render_button(frame, area, node, hit_map);
        }
        UiNodeKind::Dialog => render_dialog(frame, area, node, hit_map),
        UiNodeKind::Form | UiNodeKind::FormSection => {
            render_children_or_text(frame, area, node, hit_map, "form");
        }
        UiNodeKind::FormField => render_form_field(frame, area, node, hit_map),
        UiNodeKind::TextInput
        | UiNodeKind::Textarea
        | UiNodeKind::Checkbox
        | UiNodeKind::Select => render_input(frame, area, node, hit_map),
        UiNodeKind::SelectOption => {
            render_label(frame, area, node, hit_map, "option", HitRole::Text)
        }
        UiNodeKind::TerminalView => render_terminal_view(frame, area, node, hit_map),
        UiNodeKind::ConnectionCodeView => render_connection_code(frame, area, node),
        UiNodeKind::Tree | UiNodeKind::TreeItem | UiNodeKind::Menu => {
            render_unsupported(frame, area, node);
        }
        UiNodeKind::Icon => render_label(frame, area, node, hit_map, "icon", HitRole::Text),
    }
}

fn render_stack(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    let direction = match prop_str(node, "direction").as_deref() {
        Some("horizontal") => Direction::Horizontal,
        _ => Direction::Vertical,
    };
    render_children(frame, area, node, hit_map, direction);
}

fn render_inline(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    render_children(frame, area, node, hit_map, Direction::Horizontal);
}

fn render_panel(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    push_hit(hit_map, node, HitRole::Panel, area, None);
    let block = Block::default()
        .title(prop_str(node, "title").unwrap_or_else(|| "panel".to_string()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_children(frame, inner, node, hit_map, Direction::Vertical);
}

fn render_text(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    push_hit(hit_map, node, HitRole::Text, area, None);
    frame.render_widget(Paragraph::new(prop_text(node, "text")), area);
}

fn render_label(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
    prefix: &str,
    role: HitRole,
) {
    push_hit(hit_map, node, role, area, None);
    let label = prop_text(node, "label");
    frame.render_widget(Paragraph::new(format!("{prefix}: {label}")), area);
}

fn render_empty_state(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode) {
    let title = prop_text(node, "title");
    let description = prop_str(node, "description").unwrap_or_default();
    frame.render_widget(Paragraph::new(format!("{title}\n{description}")), area);
}

fn render_list(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    let rows = node
        .children
        .iter()
        .enumerate()
        .flat_map(|(index, child)| list_rows_from_child(index, child, hit_map, area))
        .collect::<Vec<_>>();
    frame.render_widget(List::new(rows), area);
}

fn render_list_item(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
) {
    push_hit(hit_map, node, HitRole::ListItem, area, action_prop(node));
    frame.render_widget(Paragraph::new(node_title(node)), area);
}

fn render_table_as_list(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode) {
    let columns = node
        .props
        .get("columns")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" | ")
        })
        .unwrap_or_else(|| "table".to_string());
    frame.render_widget(Paragraph::new(format!("table: {columns}")), area);
}

fn render_button(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    push_hit(hit_map, node, HitRole::Action, area, action_prop(node));
    frame.render_widget(
        Paragraph::new(format!("[ {} ]", prop_text(node, "label")))
            .style(Style::default().add_modifier(Modifier::BOLD)),
        area,
    );
}

fn render_dialog(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    push_hit(hit_map, node, HitRole::Dialog, area, action_prop(node));
    let block = Block::default()
        .title(prop_str(node, "title").unwrap_or_else(|| "dialog".to_string()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    render_slot(frame, inner, node, hit_map, "body");
}

fn render_form_field(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
) {
    push_hit(hit_map, node, HitRole::Field, area, None);
    let schema = node
        .props
        .get("schema")
        .and_then(|value| serde_json::from_value::<UiFieldSchema>(value.clone()).ok());
    let line = schema
        .map(|schema| format!("{}: {}", schema.label, prop_text(node, "default")))
        .unwrap_or_else(|| "field".to_string());
    frame.render_widget(Paragraph::new(with_error(node, line)), area);
}

fn render_input(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    push_hit(hit_map, node, HitRole::Field, area, None);
    let label = prop_text(node, "label");
    let value = match node.kind {
        UiNodeKind::Checkbox => prop_bool(node, "checked")
            .or_else(|| prop_bool(node, "default"))
            .map(|checked| if checked { "[x]" } else { "[ ]" }.to_string())
            .unwrap_or_else(|| "[ ]".to_string()),
        UiNodeKind::Select => {
            selected_option_label(node).unwrap_or_else(|| prop_text(node, "selected"))
        }
        UiNodeKind::Textarea => format!("{} (read-only)", prop_text(node, "value")),
        _ => prop_text(node, "value"),
    };
    frame.render_widget(
        Paragraph::new(with_error(node, format!("{label}: {value}"))),
        area,
    );
}

fn render_terminal_view(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
) {
    push_hit(
        hit_map,
        node,
        HitRole::TerminalView,
        area,
        Some(terminal_focus_action(node)),
    );
    frame.render_widget(
        Paragraph::new(format!(
            "terminal: {}",
            prop_str(node, "session_id").unwrap_or_default()
        ))
        .block(
            Block::default()
                .title(prop_str(node, "title").unwrap_or_else(|| "terminal".to_string()))
                .borders(Borders::ALL),
        )
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_connection_code(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode) {
    frame.render_widget(
        Paragraph::new(format!("connection code: {}", prop_text(node, "code"))),
        area,
    );
}

fn render_unsupported(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode) {
    frame.render_widget(
        Paragraph::new(format!("unsupported: {}", primitive_name(node.kind))),
        area,
    );
}

fn render_children_or_text(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
    label: &str,
) {
    if node.children.is_empty() && node.slots.is_empty() {
        frame.render_widget(Paragraph::new(label.to_string()), area);
    } else {
        render_children(frame, area, node, hit_map, Direction::Vertical);
    }
}

fn render_children(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
    direction: Direction,
) {
    let children = expanded_children(node);
    if children.is_empty() {
        return;
    }

    let chunks = Layout::default()
        .direction(direction)
        .constraints(even_constraints(children.len()))
        .split(area);

    for (child, chunk) in children.into_iter().zip(chunks.iter()) {
        render_node_inner(frame, *chunk, &child, hit_map);
    }
}

fn render_slot(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
    slot: &str,
) {
    let children = node
        .slots
        .get(slot)
        .into_iter()
        .flatten()
        .filter_map(static_child_node)
        .cloned()
        .collect::<Vec<_>>();
    if children.is_empty() {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(even_constraints(children.len()))
        .split(area);
    for (child, chunk) in children.iter().zip(chunks.iter()) {
        render_node_inner(frame, *chunk, child, hit_map);
    }
}

fn expanded_children(node: &UiNode) -> Vec<UiNode> {
    node.children
        .iter()
        .flat_map(|child| match child {
            UiChild::Node(node) => vec![*node.clone()],
            UiChild::Conditional(_) => Vec::new(),
            UiChild::BindIf(_) => Vec::new(),
            UiChild::BindList(bind_list) => match bind_list {
                botster_core::ui::UiBindList::BindList { empty_template, .. } => empty_template
                    .as_ref()
                    .map(|node| vec![*node.clone()])
                    .unwrap_or_default(),
            },
        })
        .collect()
}

fn list_rows_from_child<'a>(
    index: usize,
    child: &'a UiChild,
    hit_map: &mut HitMap,
    list_area: Rect,
) -> Vec<ListItem<'a>> {
    match child {
        UiChild::Node(node) if node.kind == UiNodeKind::ListItem => {
            let row = list_area
                .y
                .saturating_add(u16::try_from(index).unwrap_or(u16::MAX));
            if row < list_area.y.saturating_add(list_area.height) {
                push_hit(
                    hit_map,
                    node,
                    HitRole::ListItem,
                    Rect::new(list_area.x, row, list_area.width, 1),
                    action_prop(node),
                );
            }
            vec![ListItem::new(node_title(node))]
        }
        UiChild::BindList(bind_list) => match bind_list {
            botster_core::ui::UiBindList::BindList { empty_template, .. } => empty_template
                .as_ref()
                .map(|node| vec![ListItem::new(node_title(node))])
                .unwrap_or_else(|| vec![ListItem::new("bound list: waiting for entities")]),
        },
        _ => Vec::new(),
    }
}

fn static_child_node(child: &UiChild) -> Option<&UiNode> {
    match child {
        UiChild::Node(node) => Some(node),
        _ => None,
    }
}

fn node_title(node: &UiNode) -> String {
    if let Some(children) = node.slots.get("title") {
        return children
            .iter()
            .filter_map(static_child_node)
            .map(|child| prop_text(child, "text"))
            .collect::<Vec<_>>()
            .join(" ");
    }
    prop_str(node, "title")
        .or_else(|| prop_str(node, "label"))
        .or_else(|| prop_str(node, "text"))
        .unwrap_or_else(|| primitive_name(node.kind).to_string())
}

fn prop_text(node: &UiNode, name: &str) -> String {
    prop_str(node, name).unwrap_or_default()
}

fn prop_str(node: &UiNode, name: &str) -> Option<String> {
    let value = node.props.get(name)?;
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("$bind")
                .and_then(Value::as_str)
                .map(|path| format!("bind {path}"))
        })
        .or_else(|| {
            if value.is_null() {
                None
            } else {
                Some(value.to_string())
            }
        })
}

fn prop_bool(node: &UiNode, name: &str) -> Option<bool> {
    node.props.get(name).and_then(Value::as_bool)
}

fn action_prop(node: &UiNode) -> Option<UiAction> {
    node.props
        .get("action")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
}

fn terminal_focus_action(node: &UiNode) -> UiAction {
    UiAction {
        id: UiActionId("botster.terminal.focus".to_string()),
        payload: node_id(node).map(|id| json!({ "node_id": id })),
        disabled: false,
    }
}

fn selected_option_label(node: &UiNode) -> Option<String> {
    let selected = node
        .props
        .get("selected")
        .or_else(|| node.props.get("value"))
        .or_else(|| node.props.get("default"))?;
    let options = node.slots.get("options")?;

    options
        .iter()
        .filter_map(static_child_node)
        .find_map(|option| {
            let value = option.props.get("value")?;
            if value == selected {
                prop_str(option, "label")
            } else {
                None
            }
        })
}

fn with_error(node: &UiNode, line: String) -> String {
    match prop_str(node, "error") {
        Some(error) if !error.is_empty() => format!("{line}\nerror: {error}"),
        _ => line,
    }
}

fn dispatch_mouse(mouse: MouseEvent, hit_map: &HitMap) -> InputDispatch {
    let Some(region) = hit_map.lookup(mouse.column, mouse.row) else {
        return InputDispatch::Ignored;
    };

    match mouse.kind {
        MouseEventKind::Moved | MouseEventKind::Drag(_) => InputDispatch::Hover {
            node_id: region.node_id.clone(),
        },
        MouseEventKind::ScrollDown => InputDispatch::Scroll {
            node_id: region.node_id.clone(),
            lines: 3,
        },
        MouseEventKind::ScrollUp => InputDispatch::Scroll {
            node_id: region.node_id.clone(),
            lines: -3,
        },
        MouseEventKind::Down(MouseButton::Left) => region
            .action
            .as_ref()
            .map(|action| InputDispatch::Action(action_request(region, action)))
            .unwrap_or(InputDispatch::Ignored),
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

    InputDispatch::TerminalForward {
        node_id: terminal.node_id.clone(),
        bytes,
    }
}

fn action_request(region: &HitRegion, action: &UiAction) -> UiActionRequest {
    UiActionRequest {
        request_id: RequestId(format!("req-{}", region.node_id)),
        surface_id: UiSurfaceId(DEMO_SURFACE_ID.to_string()),
        action_id: action.id.clone(),
        node_id: Some(UiNodeId(region.node_id.clone())),
        kind: UiActionKind::Submit,
        values: Some(UiFormValues(Map::new())),
        payload: action.payload.clone(),
    }
}

fn push_hit(
    hit_map: &mut HitMap,
    node: &UiNode,
    role: HitRole,
    rect: Rect,
    action: Option<UiAction>,
) {
    if let Some(node_id) = node_id(node) {
        hit_map.push(HitRegion {
            node_id,
            role,
            rect,
            action,
        });
    }
}

fn node_id(node: &UiNode) -> Option<String> {
    node.id.as_ref().map(|id| id.0.clone())
}

fn primitive_name(kind: UiNodeKind) -> &'static str {
    match kind {
        UiNodeKind::Stack => "stack",
        UiNodeKind::Inline => "inline",
        UiNodeKind::Form => "form",
        UiNodeKind::FormSection => "form_section",
        UiNodeKind::FormField => "form_field",
        UiNodeKind::Panel => "panel",
        UiNodeKind::ScrollArea => "scroll_area",
        UiNodeKind::Text => "text",
        UiNodeKind::Icon => "icon",
        UiNodeKind::Badge => "badge",
        UiNodeKind::StatusDot => "status_dot",
        UiNodeKind::EmptyState => "empty_state",
        UiNodeKind::List => "list",
        UiNodeKind::ListItem => "list_item",
        UiNodeKind::Tree => "tree",
        UiNodeKind::TreeItem => "tree_item",
        UiNodeKind::Table => "table",
        UiNodeKind::Button => "button",
        UiNodeKind::IconButton => "icon_button",
        UiNodeKind::Menu => "menu",
        UiNodeKind::MenuItem => "menu_item",
        UiNodeKind::Dialog => "dialog",
        UiNodeKind::TextInput => "text_input",
        UiNodeKind::Textarea => "textarea",
        UiNodeKind::Checkbox => "checkbox",
        UiNodeKind::Select => "select",
        UiNodeKind::SelectOption => "select_option",
        UiNodeKind::TerminalView => "terminal_view",
        UiNodeKind::ConnectionCodeView => "connection_code_view",
    }
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

trait UiNodeBuilder {
    fn with_children(self, children: Vec<UiChild>) -> UiNode;
}

impl UiNodeBuilder for UiNode {
    fn with_children(mut self, children: Vec<UiChild>) -> UiNode {
        self.children = children;
        self
    }
}

fn node(kind: UiNodeKind, id: &str, props: Value) -> UiNode {
    UiNode {
        kind,
        id: Some(UiNodeId(id.to_string())),
        props: props.as_object().cloned().unwrap_or_default(),
        children: Vec::new(),
        slots: BTreeMap::new(),
    }
}

fn child(node: UiNode) -> UiChild {
    UiChild::Node(Box::new(node))
}

#[allow(dead_code)]
fn select_option(value: Value, label: &str) -> UiChild {
    child(node(
        UiNodeKind::SelectOption,
        label,
        json!({ "value": value, "label": label }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use botster_core_test_support::ui_conformance::{
        assert_ui_renderer_conformance_fixture, ui_renderer_conformance_fixtures,
    };

    #[test]
    fn renders_core_conformance_fixtures() {
        for fixture in ui_renderer_conformance_fixtures() {
            assert_ui_renderer_conformance_fixture(&fixture);
            for node in fixture.nodes {
                let (lines, _hit_map) = render_to_lines(&node, 80, 24);
                let frame = lines.join("\n");
                assert!(
                    frame.chars().any(|character| !character.is_whitespace()),
                    "fixture {} should render non-blank output",
                    fixture.name
                );
                for expected in expected_fixture_fragments(fixture.name) {
                    assert!(
                        frame.contains(expected),
                        "fixture {} should render `{expected}` in:\n{frame}",
                        fixture.name
                    );
                }
            }
        }
    }

    #[test]
    fn registry_uses_shared_primitive_names() {
        let registry = primitive_registry();
        for primitive in [
            "stack",
            "inline",
            "panel",
            "scroll_area",
            "text",
            "badge",
            "status_dot",
            "empty_state",
            "list",
            "list_item",
            "table",
            "button",
            "dialog",
            "text_input",
            "textarea",
            "checkbox",
            "select",
        ] {
            assert!(registry.contains_key(primitive), "{primitive} missing");
        }
        assert_eq!(registry["stack"], "Layout");
        assert_eq!(registry["panel"], "Block");
        assert_eq!(registry["list"], "List");
        assert_eq!(registry["table"], "Paragraph(table_as_list)");
        assert_eq!(registry["text_input"], "Paragraph");
    }

    #[test]
    fn renders_forms_field_errors_and_read_only_textarea() {
        let root = form_fixture();
        let (lines, hit_map) = render_to_lines(&root, 80, 24);
        let frame = lines.join("\n");

        assert!(frame.contains("Title: Owner authored"));
        assert!(frame.contains("Description: Line one (read-only)"));
        assert!(frame.contains("Done: [x]"));
        assert!(frame.contains("Status: Open"));
        assert!(frame.contains("error: Title is required"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.role == HitRole::Field && region.node_id == "field-title")
        );
    }

    #[test]
    fn button_hit_region_emits_core_action_request_with_semantic_id() {
        let root = action_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);

        let dispatch = dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 0,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );

        let InputDispatch::Action(request) = dispatch else {
            panic!("click should emit a core action request");
        };
        assert_eq!(request.action_id.0, "botster.session.select");
        assert_eq!(
            request.node_id,
            Some(UiNodeId("select-session".to_string()))
        );
        assert_eq!(request.kind, UiActionKind::Submit);
        assert_eq!(
            request.payload,
            Some(json!({ "session_id": "session-alpha" }))
        );
    }

    #[test]
    fn hit_map_uses_stable_core_node_ids() {
        let root = list_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);

        assert_eq!(
            hit_map.lookup(1, 0).map(|region| region.node_id.as_str()),
            Some("session-alpha")
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .all(|region| !region.node_id.is_empty())
        );
    }

    #[test]
    fn hit_map_positions_list_items_per_list_not_globally() {
        let root = two_list_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 8);

        assert_eq!(
            hit_map.lookup(1, 0).map(|region| region.node_id.as_str()),
            Some("first-list-alpha")
        );
        assert_eq!(
            hit_map.lookup(1, 4).map(|region| region.node_id.as_str()),
            Some("second-list-alpha")
        );
    }

    #[test]
    fn terminal_focus_and_input_forwarding_are_separate_paths() {
        let root = node(
            UiNodeKind::TerminalView,
            "terminal-main",
            json!({ "session_id": "session-alpha", "title": "Shell" }),
        );
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);

        let focus = dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        let InputDispatch::Action(request) = focus else {
            panic!("terminal click should emit focus action");
        };
        assert_eq!(request.action_id.0, "botster.terminal.focus");

        assert_eq!(
            dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
                &hit_map,
            ),
            InputDispatch::TerminalForward {
                node_id: "terminal-main".to_string(),
                bytes: vec![10],
            }
        );
    }

    #[test]
    fn unsupported_primitives_render_safe_fallback() {
        let root = node(UiNodeKind::Tree, "tree-fixture", json!({}));
        let (lines, _hit_map) = render_to_lines(&root, 80, 5);

        assert!(lines.join("\n").contains("unsupported: tree"));
    }

    #[test]
    fn table_renders_as_list_fallback() {
        let root = node(
            UiNodeKind::Table,
            "table-fixture",
            json!({ "columns": ["name", "status"] }),
        );
        let (lines, _hit_map) = render_to_lines(&root, 80, 5);

        assert!(lines.join("\n").contains("table: name | status"));
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

    fn list_fixture() -> UiNode {
        let mut alpha = node(
            UiNodeKind::ListItem,
            "session-alpha",
            json!({ "value": "alpha" }),
        );
        alpha.slots.insert(
            "title".to_string(),
            vec![child(node(
                UiNodeKind::Text,
                "session-alpha-title",
                json!({ "text": "alpha" }),
            ))],
        );

        let mut root = node(
            UiNodeKind::List,
            "session-list",
            json!({ "aria_label": "Sessions" }),
        );
        root.children = vec![child(alpha)];
        root.validate().expect("list fixture should validate");
        root
    }

    fn two_list_fixture() -> UiNode {
        let mut root = node(
            UiNodeKind::Stack,
            "two-list-root",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(one_row_list("first-list", "first-list-alpha", "alpha")),
            child(one_row_list("second-list", "second-list-alpha", "alpha")),
        ];
        root.validate().expect("two-list fixture should validate");
        root
    }

    fn one_row_list(list_id: &str, row_id: &str, label: &str) -> UiNode {
        let mut row = node(UiNodeKind::ListItem, row_id, json!({ "value": row_id }));
        row.slots.insert(
            "title".to_string(),
            vec![child(node(
                UiNodeKind::Text,
                &format!("{row_id}-title"),
                json!({ "text": label }),
            ))],
        );

        let mut list = node(UiNodeKind::List, list_id, json!({ "aria_label": list_id }));
        list.children = vec![child(row)];
        list
    }

    fn expected_fixture_fragments(name: &str) -> &'static [&'static str] {
        match name {
            "primitives" => &[
                "Renderer primitives",
                "badge: Ready",
                "table: name | status",
                "terminal: session-fixture",
                "connection code: pair-fixture",
            ],
            "forms" => &["Title: Owner authored", "Status: open"],
            "bindings" => &["No tickets"],
            "responsive_fallbacks" => &[
                "Confirm",
                "Body",
                "table: name",
                "terminal: session-fixture",
                "connection code: pair-fixture",
            ],
            "action_metadata" => &["[ Advance ]"],
            _ => &[],
        }
    }

    fn action_fixture() -> UiNode {
        let root = node(
            UiNodeKind::Button,
            "select-session",
            json!({
                "label": "Select",
                "action": {
                    "id": "botster.session.select",
                    "payload": { "session_id": "session-alpha" }
                }
            }),
        );
        root.validate().expect("action fixture should validate");
        root
    }

    fn form_fixture() -> UiNode {
        let mut select = node(
            UiNodeKind::Select,
            "field-status",
            json!({ "name": "status", "label": "Status", "selected": "open" }),
        );
        select.slots.insert(
            "options".to_string(),
            vec![
                child(node(
                    UiNodeKind::SelectOption,
                    "status-open",
                    json!({ "value": "open", "label": "Open" }),
                )),
                child(node(
                    UiNodeKind::SelectOption,
                    "status-closed",
                    json!({ "value": "closed", "label": "Closed" }),
                )),
            ],
        );

        let mut root = node(
            UiNodeKind::Stack,
            "form-root",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(node(
                UiNodeKind::TextInput,
                "field-title",
                json!({
                    "name": "title",
                    "label": "Title",
                    "value": "Owner authored",
                    "error": "Title is required"
                }),
            )),
            child(node(
                UiNodeKind::Textarea,
                "field-description",
                json!({ "name": "description", "label": "Description", "value": "Line one" }),
            )),
            child(node(
                UiNodeKind::Checkbox,
                "field-done",
                json!({ "name": "done", "label": "Done", "checked": true }),
            )),
            child(select),
        ];
        root.validate().expect("form fixture should validate");
        root
    }
}
