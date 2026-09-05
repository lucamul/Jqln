//! The binder pane: the document tree down the left of the editor view.

use super::{ACCENT, DIM, TRASH_WARN};
use crate::app::{App, Focus};
use crate::project::{Kind, TRASH};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

pub(super) fn draw_binder(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Binder;
    let rows = app.rows();

    let items: Vec<ListItem> = rows
        .iter()
        .map(|(id, depth)| {
            let node = &app.project.nodes[id];
            let has_kids = !app
                .project
                .children
                .get(id)
                .map(|c| c.is_empty())
                .unwrap_or(true);

            // The Trash node gets its own row: an icon, a count, and a yellow
            // nudge once it fills up.
            if id == TRASH {
                let n = app.project.trash_count();
                let colour = if n >= TRASH_WARN { Color::Yellow } else { DIM };
                let marker = if node.collapsed { "▸ " } else { "▾ " };
                return ListItem::new(Line::from(vec![
                    Span::styled("  ".repeat(*depth), Style::default()),
                    Span::styled(marker, Style::default().fg(colour)),
                    Span::styled(
                        format!("Trash · {n}"),
                        Style::default().fg(colour).add_modifier(Modifier::BOLD),
                    ),
                ]));
            }

            let in_trash = app.project.is_trashed(id);

            let marker = match (node.kind, has_kids, node.collapsed) {
                (Kind::Folder, true, true) => "▸ ",
                (Kind::Folder, true, false) => "▾ ",
                (Kind::Folder, false, _) => "▫ ",
                (Kind::Text, _, _) => "· ",
            };

            let indent = "  ".repeat(*depth);
            let mut style = Style::default();
            if node.kind == Kind::Folder {
                style = style.add_modifier(Modifier::BOLD);
            }
            if !node.include || in_trash {
                style = style.fg(DIM).add_modifier(Modifier::DIM);
            }

            let mut spans = vec![
                Span::styled(indent, Style::default()),
                Span::styled(marker, Style::default().fg(DIM)),
                Span::styled(node.title.clone(), style),
            ];
            if app.project.has_note(id) {
                spans.push(Span::styled(" ✎", Style::default().fg(DIM)));
            }
            if !node.include && !in_trash {
                spans.push(Span::styled("  ○", Style::default().fg(DIM)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let border = if focused { ACCENT } else { DIM };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {} ", app.project.meta.name),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    app.pane_binder = block.inner(area);
    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(if focused { ACCENT } else { DIM })
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );

    if rows.is_empty() {
        app.binder_state.select(None);
    } else {
        app.binder_state.select(Some(app.sel.min(rows.len() - 1)));
    }
    let mut state = std::mem::take(&mut app.binder_state);
    f.render_stateful_widget(list, area, &mut state);
    app.binder_state = state;
}
