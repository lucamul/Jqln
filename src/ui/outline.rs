//! The outline: every document as a row with word count, status and compile flag.

use super::{ACCENT, DIM};
use crate::app::App;
use crate::project::Kind;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};

/// The outline: every node as a row, with the metadata that matters for
/// judging shape — word count, status, and whether it will be compiled.
pub(super) fn draw_outline(f: &mut Frame, app: &mut App, area: Rect) {
    app.flush(); // so word counts reflect unsaved typing

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {} — outline ", app.project.meta.name),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = app.rows();
    let mut items: Vec<ListItem> = Vec::with_capacity(rows.len());
    for (id, depth) in &rows {
        let words = app.project.word_count(id);
        let noted = app.project.has_note(id);
        let node = &app.project.nodes[id];

        let indent = "  ".repeat(*depth);
        let mut title_style = Style::default();
        if node.kind == Kind::Folder {
            title_style = title_style.add_modifier(Modifier::BOLD);
        }
        if !node.include {
            title_style = title_style.fg(DIM).add_modifier(Modifier::DIM);
        }

        let title = format!("{indent}{}", node.title);
        let width = inner.width as usize;
        let meta = format!(
            "{:>7}  {:<10} {} {}",
            if node.kind == Kind::Text { format!("{words} w") } else { String::new() },
            node.status.chars().take(10).collect::<String>(),
            if noted { "✎" } else { " " },
            if node.include { "✓" } else { "·" },
        );
        let pad = width.saturating_sub(title.chars().count() + meta.chars().count());

        items.push(ListItem::new(Line::from(vec![
            Span::styled(title, title_style),
            Span::raw(" ".repeat(pad)),
            Span::styled(meta, Style::default().fg(DIM)),
        ])));
    }

    let list = List::new(items).highlight_style(
        Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
    );
    app.pane_outline = inner;
    if rows.is_empty() {
        app.outline_state.select(None);
    } else {
        app.outline_state.select(Some(app.sel.min(rows.len() - 1)));
    }
    let mut state = std::mem::take(&mut app.outline_state);
    f.render_stateful_widget(list, inner, &mut state);
    app.outline_state = state;
}
