//! Mouse handling: translating a click or a wheel tick into a selection, a
//! cursor move, or a scroll of whichever pane is under the pointer.

use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

impl App {
    pub fn on_mouse(&mut self, ev: MouseEvent) {
        if self.modal != Modal::None {
            return; // Modals are keyboard-only.
        }
        let pos = Position { x: ev.column, y: ev.row };
        match ev.kind {
            MouseEventKind::ScrollDown => self.scroll_at(pos, 1),
            MouseEventKind::ScrollUp => self.scroll_at(pos, -1),
            MouseEventKind::Down(MouseButton::Left) => self.click_at(pos),
            MouseEventKind::Drag(MouseButton::Left) => self.drag_at(pos),
            MouseEventKind::Up(MouseButton::Left) => self.release_drag(),
            _ => {}
        }
    }

    /// Extend the editor selection to the dragged-to point. Dragging only means
    /// anything inside the text: a drag that strays over the binder clamps to
    /// the editor's edge rather than selecting the tree as well.
    fn drag_at(&mut self, pos: Position) {
        if self.view != View::Editor || self.focus != Focus::Editor {
            return;
        }
        let Some(id) = self.editor_doc() else { return };
        self.ensure_editor(&id);

        // The point to hit-test, in whatever coordinate space the focused
        // editor was rendered in, clamped so an over-run drag lands on an edge.
        let target = if self.continuous {
            let inner = self.flow_inner;
            if inner.width == 0 {
                return;
            }
            let x = pos.x.clamp(inner.x, inner.x + inner.width - 1);
            let y = pos.y.clamp(inner.y, inner.y + inner.height.saturating_sub(1));
            Position {
                x: x.saturating_sub(inner.x),
                y: y.saturating_sub(inner.y)
                    .saturating_add(self.scroll)
                    .saturating_sub(self.flow_span_start),
            }
        } else {
            let pane = self.pane_editor;
            if pane.width == 0 {
                return;
            }
            Position {
                x: pos.x.clamp(pane.x, pane.x + pane.width - 1),
                y: pos.y.clamp(pane.y, pane.y + pane.height.saturating_sub(1)),
            }
        };

        if let Some(ta) = self.editors.get_mut(&id) {
            if ta.selection_range().is_none() {
                ta.start_selection();
            }
            if let Some((row, col)) = ta.cursor_at_position(target) {
                ta.move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
            }
        }
    }

    /// End of a drag: a click that never moved leaves an empty selection, which
    /// would otherwise make the next Ctrl-B wrap nothing.
    fn release_drag(&mut self) {
        let Some(id) = self.editor_doc() else { return };
        if let Some(ta) = self.editors.get_mut(&id)
            && let Some((a, b)) = ta.selection_range()
            && a == b
        {
            ta.cancel_selection();
        }
    }

    fn move_sel(&mut self, delta: isize) {
        let n = self.rows().len();
        if n == 0 {
            return;
        }
        let next = (self.sel as isize + delta).clamp(0, n as isize - 1);
        self.sel = next as usize;
    }

    fn scroll_at(&mut self, pos: Position, dir: isize) {
        match self.view {
            View::Outliner => self.move_sel(dir * 3),
            View::Corkboard => {
                if dir > 0 {
                    self.card_scroll = self.card_scroll.saturating_add(1);
                } else {
                    self.card_scroll = self.card_scroll.saturating_sub(1);
                }
            }
            View::Editor => {
                if self.pane_binder.contains(pos) {
                    self.move_sel(dir * 3);
                } else if self.continuous {
                    self.scroll = if dir > 0 {
                        self.scroll.saturating_add(3)
                    } else {
                        self.scroll.saturating_sub(3)
                    };
                } else if let Some(id) = self.editor_doc() {
                    self.ensure_editor(&id);
                    if let Some(ta) = self.editors.get_mut(&id) {
                        ta.scroll(((dir * 3) as i16, 0));
                    }
                }
            }
        }
    }

    fn click_at(&mut self, pos: Position) {
        match self.view {
            View::Outliner => {
                if self.pane_outline.contains(pos) {
                    let row = self.outline_state.offset() + (pos.y - self.pane_outline.y) as usize;
                    if row < self.rows().len() {
                        self.sel = row;
                    }
                }
            }
            View::Corkboard => {
                if let Some((id, _)) = self.card_hits.iter().find(|(_, r)| r.contains(pos)) {
                    let id = id.clone();
                    self.select_id(&id);
                }
            }
            View::Editor => {
                if self.pane_binder.contains(pos) {
                    let row = self.binder_state.offset() + (pos.y - self.pane_binder.y) as usize;
                    if row < self.rows().len() {
                        self.sel = row;
                        self.focus = Focus::Binder;
                    }
                } else if self.continuous {
                    self.click_in_flow(pos);
                } else if self.pane_editor.contains(pos) {
                    self.focus = Focus::Editor;
                    if let Some(id) = self.editor_doc() {
                        self.ensure_editor(&id);
                        if let Some(ta) = self.editors.get_mut(&id) {
                            // A fresh press drops any prior selection; a drag
                            // that follows starts a new one from here.
                            ta.cancel_selection();
                            if let Some((row, col)) = ta.cursor_at_position(pos) {
                                ta.move_cursor(tui_textarea::CursorMove::Jump(
                                    row as u16,
                                    col as u16,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    /// Continuous mode draws its documents into an offscreen buffer, so a
    /// click has to be mapped back through the same translation the blit used
    /// before the widget can hit-test it.
    fn click_in_flow(&mut self, pos: Position) {
        let Some((id, _)) = self.flow_hits.iter().find(|(_, r)| r.contains(pos)) else {
            return;
        };
        let id = id.clone();
        self.select_id(&id);
        self.focus = Focus::Editor;

        let inner = self.flow_inner;
        let offscreen = Position {
            x: pos.x.saturating_sub(inner.x),
            y: (pos.y.saturating_sub(inner.y))
                .saturating_add(self.scroll)
                .saturating_sub(self.flow_span_start),
        };
        if let Some(ta) = self.editors.get_mut(&id) {
            ta.cancel_selection();
            if let Some((row, col)) = ta.cursor_at_position(offscreen) {
                ta.move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
            }
        }
    }
}
