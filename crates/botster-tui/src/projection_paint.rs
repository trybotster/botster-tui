//! TUI-owned Ghostty projection paint.
//!
//! Kit TerminalView keeps hit regions and chrome. Styled cells come from
//! Core `GhosttyClientProjection` and are painted here after kit draw via
//! `frame.render_widget` on the HitMap `tui-terminal` inner rect.

use botster_terminal_ghostty::{ProjectedCell, ViewportProjection};
use botster_tui_kit::{HitMap, HitRole, terminal_inner_rect};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Ratatui widget that paints one Core viewport projection into a buffer.
pub struct ProjectionWidget<'a> {
    pub projection: &'a ViewportProjection,
}

impl Widget for ProjectionWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let cols = self.projection.cols as usize;
        let rows = self.projection.rows as usize;
        if cols == 0 || rows == 0 || self.projection.cells.len() < cols.saturating_mul(rows) {
            return;
        }

        let paint_cols = (area.width as usize).min(cols);
        let paint_rows = (area.height as usize).min(rows);
        for row in 0..paint_rows {
            for col in 0..paint_cols {
                let cell = &self.projection.cells[row * cols + col];
                let x = area.x + col as u16;
                let y = area.y + row as u16;
                let buffer_cell = &mut buf[(x, y)];
                let symbol = projected_symbol(cell);
                buffer_cell.set_symbol(&symbol);
                buffer_cell.set_style(projected_style(cell));
            }
        }

        let cursor = &self.projection.cursor;
        if cursor.visible && cursor.in_viewport {
            let cx = cursor.x as usize;
            let cy = cursor.y as usize;
            if cx < paint_cols && cy < paint_rows {
                let x = area.x + cursor.x;
                let y = area.y + cursor.y;
                let buffer_cell = &mut buf[(x, y)];
                let style = buffer_cell.style().add_modifier(Modifier::REVERSED);
                buffer_cell.set_style(style);
            }
        }
    }
}

fn projected_symbol(cell: &ProjectedCell) -> String {
    if cell.grapheme.is_empty() {
        " ".to_string()
    } else {
        cell.grapheme.clone()
    }
}

fn projected_style(cell: &ProjectedCell) -> Style {
    let mut style = Style::default()
        .fg(Color::Rgb(cell.fg.r, cell.fg.g, cell.fg.b))
        .bg(Color::Rgb(cell.bg.r, cell.bg.g, cell.bg.b));
    let mut modifiers = Modifier::empty();
    if cell.bold {
        modifiers |= Modifier::BOLD;
    }
    if cell.italic {
        modifiers |= Modifier::ITALIC;
    }
    if cell.underline {
        modifiers |= Modifier::UNDERLINED;
    }
    if cell.inverse {
        modifiers |= Modifier::REVERSED;
    }
    if cell.faint {
        modifiers |= Modifier::DIM;
    }
    if cell.strikethrough {
        modifiers |= Modifier::CROSSED_OUT;
    }
    if !modifiers.is_empty() {
        style = style.add_modifier(modifiers);
    }
    style
}

/// Locate the production terminal hit region registered by kit TerminalView.
pub fn tui_terminal_region(hit_map: &HitMap) -> Option<Rect> {
    hit_map
        .regions()
        .iter()
        .rev()
        .find(|region| region.node_id == "tui-terminal" && region.role == HitRole::TerminalView)
        .map(|region| region.rect)
}

/// Paint the projection into the kit terminal chrome inner rectangle.
///
/// No-op when the terminal region is absent (error-only panel, detached
/// copy-only, etc.) so callers must not invent an outer pane rectangle.
pub fn paint_projection_on_hit_map(
    frame: &mut ratatui::Frame<'_>,
    hit_map: &HitMap,
    projection: &ViewportProjection,
) {
    let Some(outer) = tui_terminal_region(hit_map) else {
        return;
    };
    let inner = terminal_inner_rect(outer);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    frame.render_widget(ProjectionWidget { projection }, inner);
}
