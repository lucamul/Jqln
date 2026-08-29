//! The editor pane: one document, or — in continuous mode — every document in
//! the container stitched into one scrolling flow.

use super::{ACCENT, DIM};
use crate::app::{App, Focus};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

/// Continuous mode. Every document in the container is laid out head to tail
/// as one column. Rather than concatenating the text into a single buffer —
/// which would mean splitting it back apart on every save — each document
/// keeps its own editor, and the stack is composed offscreen and blitted
/// through a scrolling window.
fn draw_continuous(f: &mut Frame, app: &mut App, area: Rect) {
    use ratatui::buffer::Buffer;
    use ratatui::widgets::Widget;

    let focused_pane = app.focus == Focus::Editor;
    let border = if focused_pane { ACCENT } else { DIM };
    let container = app
        .flow_container()
        .and_then(|c| app.project.nodes.get(&c).map(|n| n.title.clone()))
        .unwrap_or_else(|| app.project.meta.name.clone());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {container} — continuous "),
            Style::default()
                .fg(if focused_pane { ACCENT } else { Color::Reset })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    app.flow_inner = inner;
    app.flow_hits.clear();
    let docs = app.continuous_docs();
    if docs.is_empty() || inner.width == 0 || inner.height == 0 {
        f.render_widget(
            Paragraph::new(Span::styled("No documents here.", Style::default().fg(DIM))),
            inner,
        );
        return;
    }
    for id in &docs {
        app.ensure_editor(id);
        app.restyle(id);
    }

    // Lay out: one title row, the wrapped body, then a blank spacer row.
    let width = inner.width;
    let mut offsets: Vec<u16> = Vec::with_capacity(docs.len());
    let mut heights: Vec<u16> = Vec::with_capacity(docs.len());
    let mut total: u16 = 0;
    for id in &docs {
        let rows = app
            .editors
            .get_mut(id)
            .map(|ta| ta.measure(width).content_rows.max(1))
            .unwrap_or(1);
        offsets.push(total);
        heights.push(rows);
        total = total.saturating_add(rows.saturating_add(2));
    }

    // Keep the document being edited on screen.
    let current = app.editor_doc();
    if let Some(cur) = &current
        && let Some(i) = docs.iter().position(|d| d == cur) {
            let start = offsets[i];
            let end = start + heights[i] + 1;
            if start < app.scroll {
                app.scroll = start;
            } else if end > app.scroll + inner.height {
                app.scroll = end.saturating_sub(inner.height);
            }
        }
    let max_scroll = total.saturating_sub(inner.height);
    app.scroll = app.scroll.min(max_scroll);
    let scroll = app.scroll;

    // Only compose the documents that intersect the viewport.
    let win_end = scroll.saturating_add(inner.height);
    let visible: Vec<usize> = (0..docs.len())
        .filter(|&i| {
            let start = offsets[i];
            let end = start + heights[i] + 2;
            end > scroll && start < win_end
        })
        .collect();
    let Some(&first) = visible.first() else { return };
    let Some(&last) = visible.last() else { return };

    let span_start = offsets[first];
    let span_end = offsets[last] + heights[last] + 2;
    let span_h = span_end.saturating_sub(span_start).max(1);
    app.flow_span_start = span_start;

    let mut off = Buffer::empty(Rect::new(0, 0, width, span_h));
    for &i in &visible {
        let y = offsets[i] - span_start;
        let id = &docs[i];
        let is_current = current.as_deref() == Some(id.as_str());

        let title = app.project.nodes.get(id).map(|n| n.title.clone()).unwrap_or_default();
        let title_style = if is_current && focused_pane {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM).add_modifier(Modifier::BOLD)
        };
        Paragraph::new(Line::from(Span::styled(title, title_style)))
            .render(Rect::new(0, y, width, 1), &mut off);

        if let Some(ta) = app.editors.get(id) {
            ta.render(Rect::new(0, y + 1, width, heights[i]), &mut off);
        }

        // Where this document's text lands on screen, for click targeting.
        let screen_y = (offsets[i] + 1).saturating_add(inner.y).saturating_sub(scroll);
        if screen_y >= inner.y && screen_y < inner.y + inner.height {
            let visible_h = heights[i].min(inner.y + inner.height - screen_y);
            app.flow_hits
                .push((id.clone(), Rect::new(inner.x, screen_y, width, visible_h)));
        }
    }

    // Blit the visible window into the frame.
    let dst = f.buffer_mut();
    for row in 0..inner.height {
        let src_y = scroll + row;
        if src_y < span_start || src_y >= span_end {
            continue;
        }
        let sy = src_y - span_start;
        for col in 0..width {
            dst[(inner.x + col, inner.y + row)] = off[(col, sy)].clone();
        }
    }
}

/// A small lighthouse for the empty pane. Jqln keeps a light on the structure
/// while you are down in the words.
fn lighthouse() -> Vec<Line<'static>> {
    let beam = Style::default().fg(Color::Yellow);
    let lamp = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
    let tower = Style::default().fg(Color::Reset);
    let trim = Style::default().fg(ACCENT);
    let sea = Style::default().fg(Color::Blue);

    let rows: Vec<(&str, Style)> = vec![
        // Every row is exactly 17 columns wide so that centring them
        // individually still lines the tower up.
        ("    \\   |   /    ", beam),
        ("     \\  |  /     ", beam),
        ("---   .---.   ---", beam),
        ("      | O |      ", lamp),
        ("---   '---'   ---", beam),
        ("     /  |  \\     ", beam),
        ("    /   |   \\    ", beam),
        ("     .-----.     ", trim),
        ("     |     |     ", tower),
        ("     | [ ] |     ", tower),
        ("     |     |     ", tower),
        ("    /=======\\    ", trim),
        ("   |         |   ", tower),
        ("   |_________|   ", tower),
        ("  ~~~~~~~~~~~~~  ", sea),
    ];
    debug_assert!(rows.iter().all(|(t, _)| t.chars().count() == 17));
    rows.into_iter()
        .map(|(t, st)| Line::from(Span::styled(t, st)).centered())
        .collect()
}

pub(super) fn draw_editor(f: &mut Frame, app: &mut App, area: Rect) {
    if app.continuous {
        draw_continuous(f, app, area);
        return;
    }
    let focused = app.focus == Focus::Editor;
    let border = if focused { ACCENT } else { DIM };

    let Some(id) = app.editor_doc() else {
        // A folder is selected: show its synopsis and contents instead.
        let title = app
            .selected_id()
            .and_then(|i| app.project.nodes.get(&i).map(|n| n.title.clone()))
            .unwrap_or_else(|| "Nothing selected".into());
        let synopsis = app
            .selected_id()
            .and_then(|i| app.project.nodes.get(&i).map(|n| n.synopsis.clone()))
            .unwrap_or_default();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let mut lines = Vec::new();
        // The lighthouse only appears when there is room for all of it.
        let art = lighthouse();
        if inner.height as usize > art.len() + 4 {
            lines.extend(art);
            lines.push(Line::from(""));
        }
        lines.push(
            Line::from(Span::styled(title, Style::default().add_modifier(Modifier::BOLD)))
                .centered(),
        );
        if !synopsis.is_empty() {
            lines.push(Line::from(Span::styled(synopsis, Style::default().fg(DIM))).centered());
        }
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                "Select a document to write · ? for help",
                Style::default().fg(DIM),
            ))
            .centered(),
        );

        app.pane_editor = inner;
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
        return;
    };

    app.ensure_editor(&id);
    app.restyle(&id);
    let node_title = app.project.nodes[&id].title.clone();
    let synopsis = app.project.nodes[&id].synopsis.clone();

    // Reserve a line for the synopsis when there is one.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            format!(" {} ", node_title),
            Style::default()
                .fg(if focused { ACCENT } else { Color::Reset })
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let text_area = if synopsis.is_empty() {
        inner
    } else {
        let [syn, rest] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                synopsis,
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )))
            .wrap(Wrap { trim: false }),
            syn,
        );
        rest
    };

    app.pane_editor = text_area;
    if let Some(ta) = app.editors.get(&id) {
        f.render_widget(ta, text_area);
    }
}
