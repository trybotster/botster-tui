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
use serde_json::{Value, json};

pub const DEMO_SURFACE_ID: &str = "botster-tui.demo";

#[derive(Clone, Debug, PartialEq)]
pub struct HitRegion {
    pub node_id: String,
    pub role: HitRole,
    pub rect: Rect,
    pub action: Option<UiAction>,
    pub field: Option<FieldBinding>,
    pub row: Option<RowBinding>,
    pub terminal_mouse_mode: bool,
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

#[derive(Clone, Debug, PartialEq)]
pub struct FieldBinding {
    pub name: String,
    pub value: Value,
    pub kind: FieldKind,
    pub options: Vec<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldKind {
    Text,
    Checkbox,
    Select,
    ReadOnly,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowBinding {
    pub group_id: String,
    pub index: usize,
    pub value: Value,
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

    #[cfg(test)]
    pub fn regions(&self) -> &[HitRegion] {
        &self.regions
    }

    pub fn focusable_regions(&self) -> impl Iterator<Item = &HitRegion> {
        self.regions.iter().filter(|region| region.is_focusable())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputDispatch {
    Hover {
        node_id: String,
    },
    Focus {
        node_id: String,
    },
    Scroll {
        node_id: String,
        lines: i16,
    },
    Action(UiActionRequest),
    TerminalForward {
        node_id: String,
        bytes: Vec<u8>,
    },
    TerminalResize {
        node_id: String,
        rows: u16,
        cols: u16,
    },
    Ignored,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InputRouter {
    hover_node_id: Option<String>,
    focused_node_id: Option<String>,
    selected_rows: BTreeMap<String, String>,
    draft_values: BTreeMap<String, Value>,
    scroll_offsets: BTreeMap<String, i16>,
}

impl InputRouter {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub fn focused_node_id(&self) -> Option<&str> {
        self.focused_node_id.as_deref()
    }

    #[cfg(test)]
    pub fn hover_node_id(&self) -> Option<&str> {
        self.hover_node_id.as_deref()
    }

    #[cfg(test)]
    pub fn selected_row(&self, group_id: &str) -> Option<&str> {
        self.selected_rows.get(group_id).map(String::as_str)
    }

    #[cfg(test)]
    pub fn draft_value(&self, field_name: &str) -> Option<&Value> {
        self.draft_values.get(field_name)
    }

    pub fn draft_values(&self) -> BTreeMap<String, Value> {
        self.draft_values.clone()
    }

    #[cfg(test)]
    pub fn scroll_offset(&self, node_id: &str) -> i16 {
        self.scroll_offsets
            .get(node_id)
            .copied()
            .unwrap_or_default()
    }

    pub fn dispatch_event(&mut self, event: Event, hit_map: &HitMap) -> InputDispatch {
        match event {
            Event::Mouse(mouse) => self.dispatch_mouse(mouse, hit_map),
            Event::Key(key) => {
                self.ensure_focus(hit_map);
                self.dispatch_key(key, hit_map)
            }
            Event::Resize(_, _) => self.dispatch_resize(hit_map),
            _ => InputDispatch::Ignored,
        }
    }

    fn dispatch_mouse(&mut self, mouse: MouseEvent, hit_map: &HitMap) -> InputDispatch {
        let Some(region) = hit_map.lookup(mouse.column, mouse.row) else {
            return InputDispatch::Ignored;
        };

        if self.focused_node_id.as_deref() == Some(region.node_id.as_str())
            && region.role == HitRole::TerminalView
            && region.terminal_mouse_mode
        {
            return terminal_mouse_forward(region, mouse);
        }

        match mouse.kind {
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                self.hover_node_id = Some(region.node_id.clone());
                InputDispatch::Hover {
                    node_id: region.node_id.clone(),
                }
            }
            MouseEventKind::ScrollDown => self.scroll(region, 3),
            MouseEventKind::ScrollUp => self.scroll(region, -3),
            MouseEventKind::Down(MouseButton::Left) => {
                if !region.is_focusable() {
                    return InputDispatch::Ignored;
                }
                self.focused_node_id = Some(region.node_id.clone());
                if region.role == HitRole::TerminalView {
                    return region
                        .action
                        .as_ref()
                        .map(|action| {
                            InputDispatch::Action(action_request(
                                region,
                                action,
                                UiActionKind::Submit,
                                &self.draft_values,
                            ))
                        })
                        .unwrap_or_else(|| InputDispatch::Focus {
                            node_id: region.node_id.clone(),
                        });
                }
                self.activate_region(region, UiActionKind::Submit)
            }
            _ => InputDispatch::Ignored,
        }
    }

    fn dispatch_key(&mut self, key: KeyEvent, hit_map: &HitMap) -> InputDispatch {
        match key.code {
            KeyCode::Tab => return self.move_focus(hit_map, 1),
            KeyCode::BackTab => return self.move_focus(hit_map, -1),
            KeyCode::Esc => {
                self.draft_values.clear();
                return self
                    .focused_region(hit_map)
                    .and_then(|region| {
                        region.action.as_ref().map(|action| {
                            action_request(region, action, UiActionKind::Cancel, &self.draft_values)
                        })
                    })
                    .map(InputDispatch::Action)
                    .unwrap_or(InputDispatch::Ignored);
            }
            _ => {}
        }

        let Some(region) = self.focused_region(hit_map) else {
            return InputDispatch::Ignored;
        };

        if region.role == HitRole::TerminalView {
            return terminal_key_forward(region, key);
        }

        match key.code {
            KeyCode::Enter => self.activate_region(region, UiActionKind::Submit),
            KeyCode::Char(' ') if region.role != HitRole::Field => {
                self.activate_region(region, UiActionKind::Submit)
            }
            KeyCode::Char('v') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.activate_region(region, UiActionKind::Validate)
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.draft_values.clear();
                self.activate_region(region, UiActionKind::Reset)
            }
            KeyCode::Down => self.move_row(hit_map, region, 1),
            KeyCode::Up => self.move_row(hit_map, region, -1),
            KeyCode::Backspace if region.role == HitRole::Field => {
                self.edit_field(region, FieldEdit::Backspace)
            }
            KeyCode::Char(character) if region.role == HitRole::Field => {
                self.edit_field(region, FieldEdit::Char(character))
            }
            _ => InputDispatch::Ignored,
        }
    }

    fn dispatch_resize(&mut self, hit_map: &HitMap) -> InputDispatch {
        let Some(region) = self.focused_region(hit_map) else {
            return InputDispatch::Ignored;
        };
        if region.role != HitRole::TerminalView {
            return InputDispatch::Ignored;
        }
        let inner = terminal_inner_rect(region.rect);
        InputDispatch::TerminalResize {
            node_id: region.node_id.clone(),
            rows: inner.height,
            cols: inner.width,
        }
    }

    fn ensure_focus(&mut self, hit_map: &HitMap) {
        let focus_still_exists = self.focused_node_id.as_deref().is_some_and(|node_id| {
            hit_map
                .focusable_regions()
                .any(|region| region.node_id == node_id)
        });
        if !focus_still_exists {
            self.focused_node_id = hit_map
                .focusable_regions()
                .next()
                .map(|region| region.node_id.clone());
        }
    }

    fn focused_region<'a>(&self, hit_map: &'a HitMap) -> Option<&'a HitRegion> {
        let node_id = self.focused_node_id.as_deref()?;
        hit_map
            .focusable_regions()
            .find(|region| region.node_id == node_id)
    }

    fn move_focus(&mut self, hit_map: &HitMap, delta: isize) -> InputDispatch {
        let focusable = hit_map.focusable_regions().collect::<Vec<_>>();
        if focusable.is_empty() {
            self.focused_node_id = None;
            return InputDispatch::Ignored;
        }

        let current = self
            .focused_node_id
            .as_deref()
            .and_then(|node_id| {
                focusable
                    .iter()
                    .position(|region| region.node_id == node_id)
            })
            .unwrap_or_default();
        let next = wrap_index(current, delta, focusable.len());
        let region = focusable[next];
        self.focused_node_id = Some(region.node_id.clone());
        InputDispatch::Focus {
            node_id: region.node_id.clone(),
        }
    }

    fn activate_region(&mut self, region: &HitRegion, kind: UiActionKind) -> InputDispatch {
        if let Some(row) = &region.row {
            self.selected_rows
                .insert(row.group_id.clone(), region.node_id.clone());
        }

        if kind == UiActionKind::Submit {
            self.commit_field_cycle(region);
        }

        region
            .action
            .as_ref()
            .map(|action| {
                InputDispatch::Action(action_request(region, action, kind, &self.draft_values))
            })
            .unwrap_or_else(|| InputDispatch::Focus {
                node_id: region.node_id.clone(),
            })
    }

    fn commit_field_cycle(&mut self, region: &HitRegion) {
        let Some(field) = &region.field else {
            return;
        };

        match field.kind {
            FieldKind::Checkbox => {
                let current = self
                    .draft_values
                    .get(&field.name)
                    .cloned()
                    .unwrap_or_else(|| field.value.clone());
                self.draft_values.insert(
                    field.name.clone(),
                    Value::Bool(!current.as_bool().unwrap_or_default()),
                );
            }
            FieldKind::Select => {
                if let Some(next) = next_option_value(
                    &field.options,
                    self.draft_values.get(&field.name).unwrap_or(&field.value),
                ) {
                    self.draft_values.insert(field.name.clone(), next);
                }
            }
            FieldKind::Text | FieldKind::ReadOnly => {}
        }
    }

    fn edit_field(&mut self, region: &HitRegion, edit: FieldEdit) -> InputDispatch {
        let Some(field) = &region.field else {
            return InputDispatch::Ignored;
        };
        if field.kind != FieldKind::Text {
            return InputDispatch::Ignored;
        }

        let mut value = self
            .draft_values
            .get(&field.name)
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value_as_text(&field.value));
        match edit {
            FieldEdit::Char(character) => value.push(character),
            FieldEdit::Backspace => {
                value.pop();
            }
        }
        self.draft_values
            .insert(field.name.clone(), Value::String(value));

        InputDispatch::Focus {
            node_id: region.node_id.clone(),
        }
    }

    fn move_row(&mut self, hit_map: &HitMap, region: &HitRegion, delta: isize) -> InputDispatch {
        let Some(row) = &region.row else {
            return InputDispatch::Ignored;
        };
        let rows = hit_map
            .focusable_regions()
            .filter(|candidate| {
                candidate
                    .row
                    .as_ref()
                    .is_some_and(|candidate_row| candidate_row.group_id == row.group_id)
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return InputDispatch::Ignored;
        }

        let current = rows
            .iter()
            .position(|candidate| candidate.node_id == region.node_id)
            .unwrap_or_default();
        let next = rows[wrap_index(current, delta, rows.len())];
        self.focused_node_id = Some(next.node_id.clone());
        self.selected_rows
            .insert(row.group_id.clone(), next.node_id.clone());
        InputDispatch::Focus {
            node_id: next.node_id.clone(),
        }
    }

    fn scroll(&mut self, region: &HitRegion, lines: i16) -> InputDispatch {
        let entry = self
            .scroll_offsets
            .entry(region.node_id.clone())
            .or_default();
        *entry = entry.saturating_add(lines);
        InputDispatch::Scroll {
            node_id: region.node_id.clone(),
            lines,
        }
    }
}

#[derive(Clone, Copy)]
enum FieldEdit {
    Char(char),
    Backspace,
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

#[cfg(test)]
pub fn dispatch_event(event: Event, hit_map: &HitMap) -> InputDispatch {
    InputRouter::new().dispatch_event(event, hit_map)
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
        ("menu", "Block"),
        ("menu_item", "Paragraph"),
        ("dialog", "Block"),
        ("text_input", "Paragraph"),
        ("textarea", "Paragraph(read_only)"),
        ("checkbox", "Paragraph"),
        ("select", "Paragraph"),
    ])
}

#[cfg(test)]
#[allow(dead_code)]
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
        UiNodeKind::Table => render_table_as_list(frame, area, node, hit_map),
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
        UiNodeKind::Menu => render_menu(frame, area, node, hit_map),
        UiNodeKind::Tree | UiNodeKind::TreeItem => render_unsupported(frame, area, node),
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
    let list_id = node_id(node).unwrap_or_else(|| "list".to_string());
    let rows = node
        .children
        .iter()
        .enumerate()
        .flat_map(|(index, child)| list_rows_from_child(index, child, hit_map, area, &list_id))
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

fn render_table_as_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
) {
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
    let mut lines = vec![format!("table: {columns}")];
    let table_id = node_id(node).unwrap_or_else(|| "table".to_string());
    if let Some(rows) = node.props.get("rows").and_then(Value::as_array) {
        for (index, row) in rows.iter().enumerate() {
            let row_id = row
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("{table_id}-row-{index}"));
            let row_y = area
                .y
                .saturating_add(u16::try_from(index + 1).unwrap_or(u16::MAX));
            if row_y < area.y.saturating_add(area.height) {
                hit_map.push(HitRegion {
                    node_id: row_id,
                    role: HitRole::ListItem,
                    rect: Rect::new(area.x, row_y, area.width, 1),
                    action: action_prop(node),
                    field: None,
                    row: Some(RowBinding {
                        group_id: table_id.clone(),
                        index,
                        value: row.clone(),
                    }),
                    terminal_mouse_mode: false,
                });
            }
            lines.push(table_row_label(row));
        }
    }
    frame.render_widget(Paragraph::new(lines.join("\n")), area);
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

fn render_menu(frame: &mut ratatui::Frame<'_>, area: Rect, node: &UiNode, hit_map: &mut HitMap) {
    let block = Block::default()
        .title(prop_str(node, "label").unwrap_or_else(|| "menu".to_string()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if node.slots.contains_key("items") {
        render_slot(frame, inner, node, hit_map, "items");
    } else {
        render_children(frame, inner, node, hit_map, Direction::Vertical);
    }
}

fn render_form_field(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    node: &UiNode,
    hit_map: &mut HitMap,
) {
    push_hit_with(
        hit_map,
        node,
        HitRole::Field,
        area,
        action_prop(node),
        field_binding(node),
        None,
    );
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
    push_hit_with(
        hit_map,
        node,
        HitRole::Field,
        area,
        action_prop(node),
        field_binding(node),
        None,
    );
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
    if let Some(node_id) = node_id(node) {
        hit_map.push(HitRegion {
            node_id,
            role: HitRole::TerminalView,
            rect: area,
            action: Some(terminal_focus_action(node)),
            field: None,
            row: None,
            terminal_mouse_mode: prop_bool(node, "mouse_mode").unwrap_or_default(),
        });
    }
    let block = Block::default()
        .title(prop_str(node, "title").unwrap_or_else(|| "terminal".to_string()))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if node.children.is_empty() && node.slots.is_empty() {
        frame.render_widget(
            Paragraph::new(format!(
                "terminal: {}",
                prop_str(node, "session_id").unwrap_or_default()
            ))
            .wrap(Wrap { trim: false }),
            inner,
        );
    } else {
        render_children(frame, inner, node, hit_map, Direction::Vertical);
    }
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
    list_id: &str,
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
                if let Some(region) = hit_map.regions.last_mut() {
                    region.row = Some(RowBinding {
                        group_id: list_id.to_string(),
                        index,
                        value: node
                            .props
                            .get("value")
                            .cloned()
                            .unwrap_or_else(|| Value::String(region.node_id.clone())),
                    });
                }
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

fn field_binding(node: &UiNode) -> Option<FieldBinding> {
    let schema = node
        .props
        .get("schema")
        .and_then(|value| serde_json::from_value::<UiFieldSchema>(value.clone()).ok());
    let name =
        prop_str(node, "name").or_else(|| schema.as_ref().map(|schema| schema.name.clone()))?;
    let kind = match node.kind {
        UiNodeKind::Checkbox => FieldKind::Checkbox,
        UiNodeKind::Select => FieldKind::Select,
        UiNodeKind::Textarea => FieldKind::ReadOnly,
        UiNodeKind::TextInput | UiNodeKind::FormField => FieldKind::Text,
        _ => return None,
    };
    let value = match kind {
        FieldKind::Checkbox => prop_bool(node, "checked")
            .or_else(|| prop_bool(node, "default"))
            .map(Value::Bool)
            .unwrap_or(Value::Bool(false)),
        FieldKind::Select => node
            .props
            .get("selected")
            .or_else(|| node.props.get("value"))
            .or_else(|| node.props.get("default"))
            .cloned()
            .unwrap_or(Value::Null),
        FieldKind::Text | FieldKind::ReadOnly => node
            .props
            .get("value")
            .or_else(|| node.props.get("default"))
            .or_else(|| schema.as_ref().and_then(|schema| schema.default.as_ref()))
            .cloned()
            .unwrap_or(Value::String(String::new())),
    };
    Some(FieldBinding {
        name,
        value,
        kind,
        options: option_values(node),
    })
}

fn option_values(node: &UiNode) -> Vec<Value> {
    node.slots
        .get("options")
        .into_iter()
        .flatten()
        .filter_map(static_child_node)
        .filter(|option| !prop_bool(option, "disabled").unwrap_or_default())
        .filter_map(|option| option.props.get("value").cloned())
        .collect()
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

fn terminal_key_forward(region: &HitRegion, key: KeyEvent) -> InputDispatch {
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
        node_id: region.node_id.clone(),
        bytes,
    }
}

fn terminal_mouse_forward(region: &HitRegion, mouse: MouseEvent) -> InputDispatch {
    let Some(button_code) = mouse_button_code(mouse.kind) else {
        return InputDispatch::Ignored;
    };
    let inner = terminal_inner_rect(region.rect);
    let column = mouse.column.saturating_sub(inner.x).saturating_add(1);
    let row = mouse.row.saturating_sub(inner.y).saturating_add(1);
    InputDispatch::TerminalForward {
        node_id: region.node_id.clone(),
        bytes: format!("\x1b[<{button_code};{column};{row}M").into_bytes(),
    }
}

fn terminal_inner_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x.saturating_add(1),
        y: rect.y.saturating_add(1),
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    }
}

fn mouse_button_code(kind: MouseEventKind) -> Option<u8> {
    match kind {
        MouseEventKind::Down(MouseButton::Left) => Some(0),
        MouseEventKind::Down(MouseButton::Middle) => Some(1),
        MouseEventKind::Down(MouseButton::Right) => Some(2),
        MouseEventKind::ScrollUp => Some(64),
        MouseEventKind::ScrollDown => Some(65),
        _ => None,
    }
}

fn action_request(
    region: &HitRegion,
    action: &UiAction,
    kind: UiActionKind,
    drafts: &BTreeMap<String, Value>,
) -> UiActionRequest {
    UiActionRequest {
        request_id: RequestId(format!("req-{}", region.node_id)),
        surface_id: UiSurfaceId(DEMO_SURFACE_ID.to_string()),
        action_id: action.id.clone(),
        node_id: Some(UiNodeId(region.node_id.clone())),
        kind,
        values: Some(UiFormValues(
            drafts
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        )),
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
    push_hit_with(hit_map, node, role, rect, action, None, None);
}

fn push_hit_with(
    hit_map: &mut HitMap,
    node: &UiNode,
    role: HitRole,
    rect: Rect,
    action: Option<UiAction>,
    field: Option<FieldBinding>,
    row: Option<RowBinding>,
) {
    if let Some(node_id) = node_id(node) {
        hit_map.push(HitRegion {
            node_id,
            role,
            rect,
            action,
            field,
            row,
            terminal_mouse_mode: false,
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

fn wrap_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let len = isize::try_from(len).unwrap_or(1);
    let current = isize::try_from(current).unwrap_or_default();
    usize::try_from((current + delta).rem_euclid(len)).unwrap_or_default()
}

fn next_option_value(options: &[Value], current: &Value) -> Option<Value> {
    if options.is_empty() {
        return None;
    }
    let current_index = options
        .iter()
        .position(|option| option == current)
        .unwrap_or_default();
    options
        .get(wrap_index(current_index, 1, options.len()))
        .cloned()
}

fn value_as_text(value: &Value) -> String {
    value.as_str().map(ToOwned::to_owned).unwrap_or_else(|| {
        if value.is_null() {
            String::new()
        } else {
            value.to_string()
        }
    })
}

fn table_row_label(row: &Value) -> String {
    match row {
        Value::Object(map) => map
            .iter()
            .filter(|(key, _)| key.as_str() != "id")
            .map(|(_, value)| value_as_text(value))
            .collect::<Vec<_>>()
            .join(" | "),
        _ => value_as_text(row),
    }
}

impl HitRegion {
    fn is_focusable(&self) -> bool {
        matches!(
            self.role,
            HitRole::Action
                | HitRole::Field
                | HitRole::ListItem
                | HitRole::Dialog
                | HitRole::TerminalView
        )
    }
}

#[cfg(test)]
trait UiNodeBuilder {
    fn with_children(self, children: Vec<UiChild>) -> UiNode;
}

#[cfg(test)]
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
            "menu",
            "menu_item",
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
    fn keyboard_focus_uses_render_order_filtered_to_focusable_regions() {
        let root = focus_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 8);
        let mut router = InputRouter::new();

        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &hit_map
            ),
            InputDispatch::Focus {
                node_id: "focus-field".to_string()
            }
        );
        assert_eq!(router.focused_node_id(), Some("focus-field"));

        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)),
                &hit_map
            ),
            InputDispatch::Focus {
                node_id: "focus-action".to_string()
            }
        );
        assert_eq!(router.focused_node_id(), Some("focus-action"));
    }

    #[test]
    fn field_editing_stays_local_until_validate_or_submit() {
        let root = node(
            UiNodeKind::TextInput,
            "field-title",
            json!({
                "name": "title",
                "label": "Title",
                "value": "Draft",
                "action": { "id": "botster.form.validate" }
            }),
        );
        let (_lines, hit_map) = render_to_lines(&root, 80, 5);
        let mut router = InputRouter::new();

        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Char('!'), KeyModifiers::NONE)),
                &hit_map
            ),
            InputDispatch::Focus {
                node_id: "field-title".to_string()
            }
        );
        assert_eq!(
            router.draft_value("title"),
            Some(&Value::String("Draft!".to_string()))
        );

        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::CONTROL)),
            &hit_map,
        );
        let InputDispatch::Action(request) = dispatch else {
            panic!("validate should emit semantic action request");
        };
        assert_eq!(request.kind, UiActionKind::Validate);
        assert_eq!(
            request.values.unwrap().0.get("title"),
            Some(&Value::String("Draft!".to_string()))
        );
    }

    #[test]
    fn checkbox_and_select_commit_through_submit_or_validate_only() {
        let mut select = node(
            UiNodeKind::Select,
            "field-status",
            json!({
                "name": "status",
                "label": "Status",
                "selected": "open",
                "action": { "id": "botster.form.submit" }
            }),
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
        let mut root = node(UiNodeKind::Stack, "fields-root", json!({}));
        root.children = vec![
            child(node(
                UiNodeKind::Checkbox,
                "field-done",
                json!({
                    "name": "done",
                    "label": "Done",
                    "checked": false,
                    "action": { "id": "botster.form.submit" }
                }),
            )),
            child(select),
        ];
        let (_lines, hit_map) = render_to_lines(&root, 80, 8);
        let mut router = InputRouter::new();

        let checkbox = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        let InputDispatch::Action(request) = checkbox else {
            panic!("checkbox submit should emit an action");
        };
        assert_eq!(request.kind, UiActionKind::Submit);
        assert_eq!(
            request.values.unwrap().0.get("done"),
            Some(&Value::Bool(true))
        );

        let select = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &hit_map,
        );
        assert_eq!(
            select,
            InputDispatch::Focus {
                node_id: "field-status".to_string()
            }
        );
        let select_submit = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        let InputDispatch::Action(request) = select_submit else {
            panic!("select submit should emit an action");
        };
        assert_eq!(request.kind, UiActionKind::Submit);
        assert_eq!(
            request.values.unwrap().0.get("status"),
            Some(&Value::String("closed".to_string()))
        );
    }

    #[test]
    fn list_keyboard_selection_is_group_local_and_activation_is_semantic() {
        let root = list_with_rows_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 8);
        let mut router = InputRouter::new();

        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
                &hit_map
            ),
            InputDispatch::Focus {
                node_id: "session-beta".to_string()
            }
        );
        assert_eq!(router.selected_row("session-list"), Some("session-beta"));

        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        let InputDispatch::Action(request) = dispatch else {
            panic!("list activation should emit row action");
        };
        assert_eq!(request.action_id.0, "botster.session.open");
        assert_eq!(request.node_id, Some(UiNodeId("session-beta".to_string())));
    }

    #[test]
    fn table_fallback_rows_add_hit_regions_and_share_selection_path() {
        let root = node(
            UiNodeKind::Table,
            "sessions-table",
            json!({
                "columns": ["name", "status"],
                "rows": [
                    { "id": "table-alpha", "name": "alpha", "status": "open" },
                    { "id": "table-beta", "name": "beta", "status": "closed" }
                ],
                "action": { "id": "botster.table.select" }
            }),
        );
        let (_lines, hit_map) = render_to_lines(&root, 80, 6);
        let mut router = InputRouter::new();

        assert_eq!(
            hit_map.lookup(1, 1).map(|region| region.node_id.as_str()),
            Some("table-alpha")
        );
        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
                &hit_map
            ),
            InputDispatch::Focus {
                node_id: "table-beta".to_string()
            }
        );
        assert_eq!(router.selected_row("sessions-table"), Some("table-beta"));
    }

    #[test]
    fn mouse_hover_updates_renderer_local_state_without_action_payload() {
        let root = action_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 5);
        let mut router = InputRouter::new();

        assert_eq!(
            router.dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Moved,
                    column: 1,
                    row: 0,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Hover {
                node_id: "select-session".to_string()
            }
        );
        assert_eq!(router.hover_node_id(), Some("select-session"));
    }

    #[test]
    fn click_on_non_focusable_region_does_not_replace_focus() {
        let root = focus_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 8);
        let mut router = InputRouter::new();

        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &hit_map
            ),
            InputDispatch::Focus {
                node_id: "focus-field".to_string()
            }
        );
        assert_eq!(
            router.dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: 0,
                    row: 0,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map
            ),
            InputDispatch::Ignored
        );
        assert_eq!(router.focused_node_id(), Some("focus-field"));
    }

    #[test]
    fn normal_mouse_scroll_updates_renderer_local_scroll_state() {
        let root = list_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);
        let mut router = InputRouter::new();

        assert_eq!(
            router.dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: 1,
                    row: 0,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map
            ),
            InputDispatch::Scroll {
                node_id: "session-alpha".to_string(),
                lines: 3,
            }
        );
        assert_eq!(router.scroll_offset("session-alpha"), 3);
    }

    #[test]
    fn overlapping_hit_regions_route_to_last_rendered_region() {
        let root = panel_with_action_child_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 5);

        assert_eq!(
            hit_map.lookup(1, 1).map(|region| region.node_id.as_str()),
            Some("panel-action")
        );
        let dispatch = dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        let InputDispatch::Action(request) = dispatch else {
            panic!("overlapping cell should route to topmost action");
        };
        assert_eq!(request.action_id.0, "botster.panel.child");
    }

    #[test]
    fn menu_renders_items_as_focusable_actions() {
        let root = menu_fixture();
        let (_lines, hit_map) = render_to_lines(&root, 80, 5);

        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "menu-open" && region.role == HitRole::Action)
        );
        let dispatch = dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        let InputDispatch::Action(request) = dispatch else {
            panic!("menu item click should emit a semantic action");
        };
        assert_eq!(request.action_id.0, "botster.menu.open");
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
    fn focused_terminal_without_mouse_mode_keeps_mouse_events_botster_owned() {
        let root = node(
            UiNodeKind::TerminalView,
            "terminal-main",
            json!({ "session_id": "session-alpha", "title": "Shell" }),
        );
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);
        let mut router = InputRouter::new();

        let focus = router.dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        assert!(matches!(focus, InputDispatch::Action(_)));

        let dispatch = router.dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 4,
                row: 3,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        assert_eq!(
            dispatch,
            InputDispatch::Scroll {
                node_id: "terminal-main".to_string(),
                lines: 3,
            }
        );
        assert_eq!(router.scroll_offset("terminal-main"), 3);
    }

    #[test]
    fn focused_terminal_mouse_mode_forwards_sgr_press_and_wheel_only() {
        let root = node(
            UiNodeKind::TerminalView,
            "terminal-main",
            json!({ "session_id": "session-alpha", "title": "Shell", "mouse_mode": true }),
        );
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);
        let mut router = InputRouter::new();

        let focus = router.dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        assert!(matches!(focus, InputDispatch::Action(_)));

        assert_eq!(
            router.dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::ScrollDown,
                    column: 4,
                    row: 3,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::TerminalForward {
                node_id: "terminal-main".to_string(),
                bytes: b"\x1b[<65;4;3M".to_vec(),
            }
        );
        assert_eq!(router.scroll_offset("terminal-main"), 0);

        assert_eq!(
            router.dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Up(MouseButton::Left),
                    column: 4,
                    row: 3,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Ignored
        );
        assert_eq!(
            router.dispatch_event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Drag(MouseButton::Left),
                    column: 4,
                    row: 3,
                    modifiers: KeyModifiers::empty(),
                }),
                &hit_map,
            ),
            InputDispatch::Ignored
        );
    }

    #[test]
    fn resize_dispatch_targets_focused_terminal_inner_rect() {
        let root = node(
            UiNodeKind::TerminalView,
            "terminal-main",
            json!({ "session_id": "session-alpha", "title": "Shell" }),
        );
        let (_lines, hit_map) = render_to_lines(&root, 80, 10);
        let mut router = InputRouter::new();

        let focus = router.dispatch_event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 1,
                row: 1,
                modifiers: KeyModifiers::empty(),
            }),
            &hit_map,
        );
        assert!(matches!(focus, InputDispatch::Action(_)));

        assert_eq!(
            router.dispatch_event(Event::Resize(120, 40), &hit_map),
            InputDispatch::TerminalResize {
                node_id: "terminal-main".to_string(),
                rows: 8,
                cols: 78,
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

    fn focus_fixture() -> UiNode {
        let mut root = node(
            UiNodeKind::Stack,
            "focus-root",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(node(
                UiNodeKind::Text,
                "focus-title",
                json!({ "text": "Title" }),
            )),
            child(node(
                UiNodeKind::Button,
                "focus-action",
                json!({ "label": "Open", "action": { "id": "botster.open" } }),
            )),
            child(node(
                UiNodeKind::TextInput,
                "focus-field",
                json!({ "name": "title", "label": "Title", "value": "Draft" }),
            )),
            child(node(
                UiNodeKind::TerminalView,
                "focus-terminal",
                json!({ "session_id": "session-alpha" }),
            )),
        ];
        root
    }

    fn panel_with_action_child_fixture() -> UiNode {
        node(UiNodeKind::Panel, "panel-root", json!({ "title": "Panel" })).with_children(vec![
            child(node(
                UiNodeKind::Button,
                "panel-action",
                json!({ "label": "Open", "action": { "id": "botster.panel.child" } }),
            )),
        ])
    }

    fn menu_fixture() -> UiNode {
        let mut menu = node(UiNodeKind::Menu, "main-menu", json!({ "label": "Actions" }));
        menu.slots.insert(
            "items".to_string(),
            vec![child(node(
                UiNodeKind::MenuItem,
                "menu-open",
                json!({ "label": "Open", "action": { "id": "botster.menu.open" } }),
            ))],
        );
        menu
    }

    fn list_with_rows_fixture() -> UiNode {
        let mut list = node(UiNodeKind::List, "session-list", json!({}));
        list.children = vec![
            child(list_row_with_action("session-alpha", "alpha")),
            child(list_row_with_action("session-beta", "beta")),
        ];
        list
    }

    fn list_row_with_action(row_id: &str, label: &str) -> UiNode {
        let mut row = node(
            UiNodeKind::ListItem,
            row_id,
            json!({
                "value": row_id,
                "action": {
                    "id": "botster.session.open",
                    "payload": { "session_id": row_id }
                }
            }),
        );
        row.slots.insert(
            "title".to_string(),
            vec![child(node(
                UiNodeKind::Text,
                &format!("{row_id}-title"),
                json!({ "text": label }),
            ))],
        );
        row
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
