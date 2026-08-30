//! The card grid: one index card per child, showing synopses instead of prose.

use super::{ACCENT, DIM};
use crate::app::App;
use crate::project::Kind;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// The card grid: one index card per child, showing the synopsis rather than
/// the prose, so structure can be judged without reading the text.
pub(super) fn draw_cards(f: &mut Frame, app: &mut App, area: Rect) {
    let container = app
        .project
        .nodes
        .get(&app.card_root)
        .map(|n| n.title.clone())
        .unwrap_or_else(|| app.project.meta.name.clone());

    let heading = if app.mouse {
        format!(" {container} — cards ")
    } else {
        // Drag-to-reorder needs the app to see the mouse.
        format!(" {container} — cards  ·  F7 to drag ")
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            heading,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    app.pane_cards = inner;
    app.card_hits.clear();
    let cards = app.cards();
    if cards.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled("Nothing here yet — press n.", Style::default().fg(DIM))),
            inner,
        );
        return;
    }

    const CARD_W: u16 = 30;
    const CARD_H: u16 = 8;
    let cols = ((inner.width / CARD_W).max(1)) as usize;
    app.card_cols = cols;
    let selected = app.selected_id();

    // Scroll by whole rows of cards, keeping the selected card in view.
    let rows_per_page = ((inner.height / CARD_H).max(1)) as usize;
    let total_rows = cards.len().div_ceil(cols);
    if let Some(sel) = &selected
        && let Some(i) = cards.iter().position(|c| c == sel) {
            let row = i / cols;
            if row < app.card_scroll {
                app.card_scroll = row;
            } else if row >= app.card_scroll + rows_per_page {
                app.card_scroll = row + 1 - rows_per_page;
            }
        }
    app.card_scroll = app.card_scroll.min(total_rows.saturating_sub(rows_per_page));
    let first = app.card_scroll * cols;
    let after_last = ((app.card_scroll + rows_per_page) * cols).min(cards.len());

    if total_rows > rows_per_page {
        let more = total_rows - (app.card_scroll + rows_per_page).min(total_rows);
        if more > 0 {
            let note = format!(" {more} more row(s) below ");
            let y = inner.y + inner.height - 1;
            f.render_widget(
                Paragraph::new(Span::styled(note, Style::default().fg(DIM))),
                Rect::new(inner.x, y, inner.width, 1),
            );
        }
    }

    for (i, id) in cards.iter().enumerate().take(after_last).skip(first) {
        let vis = i - first;
        let row = (vis / cols) as u16;
        let col = (vis % cols) as u16;
        let x = inner.x + col * CARD_W;
        let y = inner.y + row * CARD_H;
        if y + CARD_H > inner.y + inner.height {
            break;
        }
        let rect = Rect::new(x, y, CARD_W.min(inner.width), CARD_H);

        let node = &app.project.nodes[id];
        let is_sel = selected.as_deref() == Some(id.as_str());
        let is_drop_target = app.drag_card.is_some()
            && app.drag_card.as_deref() != Some(id.as_str())
            && app.drag_over.as_deref() == Some(id.as_str());
        let border = if is_drop_target {
            Color::Yellow
        } else if is_sel {
            ACCENT
        } else {
            DIM
        };

        let mut title_style = Style::default().add_modifier(Modifier::BOLD);
        if !node.include {
            title_style = title_style.fg(DIM).add_modifier(Modifier::DIM);
        }

        let mut lines = vec![Line::from(Span::styled(node.title.clone(), title_style))];
        lines.push(Line::from(""));
        if node.synopsis.is_empty() {
            lines.push(Line::from(Span::styled(
                "no synopsis",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                node.synopsis.clone(),
                Style::default().fg(Color::Reset),
            )));
        }

        let mut footer: Vec<Span> = Vec::new();
        if node.kind == Kind::Folder {
            let tag = match node.heading.as_str() {
                "" => "folder".to_string(),
                "title" => "folder · title heading".to_string(),
                name => format!("folder · “{name}”"),
            };
            footer.push(Span::styled(tag, Style::default().fg(DIM)));
        }
        if !node.status.is_empty() {
            if !footer.is_empty() {
                footer.push(Span::styled(" · ", Style::default().fg(DIM)));
            }
            footer.push(Span::styled(node.status.clone(), Style::default().fg(Color::Yellow)));
        }
        if !node.label.is_empty() {
            if !footer.is_empty() {
                footer.push(Span::styled(" · ", Style::default().fg(DIM)));
            }
            footer.push(Span::styled(node.label.clone(), Style::default().fg(Color::Magenta)));
        }
        if !node.include {
            if !footer.is_empty() {
                footer.push(Span::styled(" · ", Style::default().fg(DIM)));
            }
            footer.push(Span::styled("excluded", Style::default().fg(DIM)));
        }
        if app.project.has_note(id) {
            if !footer.is_empty() {
                footer.push(Span::styled(" · ", Style::default().fg(DIM)));
            }
            footer.push(Span::styled("✎ notes", Style::default().fg(DIM)));
        }

        app.card_hits.push((id.clone(), rect));
        let card = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border));
        let card_inner = card.inner(rect);
        f.render_widget(card, rect);

        let [text_area, foot_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(card_inner);
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);
        if !footer.is_empty() {
            f.render_widget(Paragraph::new(Line::from(footer)), foot_area);
        }
    }
}
