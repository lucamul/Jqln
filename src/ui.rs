//! All rendering. Nothing here mutates the project except to lazily open
//! editors for documents that are about to become visible.

use crate::app::{App, Focus, Modal, Prompt, View};
use crate::project::Kind;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;

pub fn draw(f: &mut Frame, app: &mut App) {
    app.sync_cursor_modes();

    let [body, status] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    match app.view {
        View::Editor => draw_editor_view(f, app, body),
        View::Corkboard => draw_cards(f, app, body),
        View::Outliner => draw_outline(f, app, body),
    }
    draw_status(f, app, status);

    match app.modal {
        Modal::None => {}
        Modal::Help => draw_help(f),
        Modal::ConfirmDelete => draw_confirm(f, app),
        Modal::Input(p) => draw_input(f, app, p),
        Modal::Results => draw_results(f, app),
        Modal::Snapshots => draw_snapshots(f, app),
    }
}

fn draw_editor_view(f: &mut Frame, app: &mut App, area: Rect) {
    let binder_width = 34u16.min(area.width.saturating_sub(20)).max(20);
    let [left, right] =
        Layout::horizontal([Constraint::Length(binder_width), Constraint::Min(20)]).areas(area);
    draw_binder(f, app, left);
    draw_editor(f, app, right);
}

fn draw_binder(f: &mut Frame, app: &mut App, area: Rect) {
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
            if !node.include {
                style = style.fg(DIM).add_modifier(Modifier::DIM);
            }

            let mut spans = vec![
                Span::styled(indent, Style::default()),
                Span::styled(marker, Style::default().fg(DIM)),
                Span::styled(node.title.clone(), style),
            ];
            if !node.include {
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

fn draw_editor(f: &mut Frame, app: &mut App, area: Rect) {
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

/// The card grid: one index card per child, showing the synopsis rather than
/// the prose, so structure can be judged without reading the text.
fn draw_cards(f: &mut Frame, app: &mut App, area: Rect) {
    let container = app
        .flow_container()
        .and_then(|c| app.project.nodes.get(&c).map(|n| n.title.clone()))
        .unwrap_or_else(|| app.project.meta.name.clone());

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {container} — cards "),
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
        let border = if is_sel { ACCENT } else { DIM };

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
            footer.push(Span::styled("folder", Style::default().fg(DIM)));
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

/// The outline: every node as a row, with the metadata that matters for
/// judging shape — word count, status, and whether it will be compiled.
fn draw_outline(f: &mut Frame, app: &mut App, area: Rect) {
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
            "{:>7}  {:<10} {}",
            if node.kind == Kind::Text { format!("{words} w") } else { String::new() },
            node.status.chars().take(10).collect::<String>(),
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

fn draw_status(f: &mut Frame, app: &mut App, area: Rect) {
    let words = app.current_words();
    let total = app.total_words();
    let session = total as isize - app.session_base as isize;
    let target = app.project.targets.project_words;

    let mut spans = vec![
        Span::styled(
            if app.dirty { " ● " } else { " ○ " },
            Style::default().fg(if app.dirty { Color::Yellow } else { DIM }),
        ),
        Span::styled(format!("{words} w"), Style::default().fg(Color::Reset)),
        Span::styled("  ·  ", Style::default().fg(DIM)),
        Span::styled(format!("{total} total"), Style::default().fg(DIM)),
    ];

    if target > 0 {
        let pct = (total as f64 / target as f64 * 100.0).round() as usize;
        spans.push(Span::styled("  ·  ", Style::default().fg(DIM)));
        spans.push(Span::styled(
            format!("{pct}% of {target}"),
            Style::default().fg(if pct >= 100 { Color::Green } else { DIM }),
        ));
    }

    spans.push(Span::styled("  ·  ", Style::default().fg(DIM)));
    spans.push(Span::styled(
        format!("{session:+} session"),
        Style::default().fg(if session > 0 { Color::Green } else { DIM }),
    ));

    if !app.status.is_empty() {
        spans.push(Span::styled("  ·  ", Style::default().fg(DIM)));
        spans.push(Span::styled(app.status.clone(), Style::default().fg(ACCENT)));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Reset)),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn draw_input(f: &mut Frame, app: &mut App, prompt: Prompt) {
    let title = match prompt {
        Prompt::NewText => " New document ",
        Prompt::NewFolder => " New folder ",
        Prompt::Rename => " Rename ",
        Prompt::Synopsis => " Synopsis ",
        Prompt::Status => " Status ",
        Prompt::Label => " Label ",
        Prompt::Keywords => " Keywords (comma separated) ",
        Prompt::Search => " Search ",
    };
    let area = centered(f.area(), 60, 3);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            title,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_results(f: &mut Frame, app: &mut App) {
    let area = centered(f.area(), 80, 20);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {} matches for \"{}\" ", app.hits.len(), app.query),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [list_area, hint] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let items: Vec<ListItem> = app
        .hits
        .iter()
        .map(|h| {
            let title = app
                .project
                .nodes
                .get(&h.id)
                .map(|n| n.title.clone())
                .unwrap_or_default();
            let where_ = if h.line == 0 {
                "title".to_string()
            } else {
                format!("L{}", h.line)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{title:<22.22} "), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("{where_:>5}  "), Style::default().fg(DIM)),
                Span::styled(h.preview.clone(), Style::default().fg(Color::Reset)),
            ]))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    if !app.hits.is_empty() {
        state.select(Some(app.hit_sel.min(app.hits.len() - 1)));
    }
    f.render_stateful_widget(list, list_area, &mut state);
    f.render_widget(
        Paragraph::new(Span::styled(
            "enter to jump · ↑↓ to move · esc to close",
            Style::default().fg(DIM),
        )),
        hint,
    );
}

fn draw_snapshots(f: &mut Frame, app: &mut App) {
    let title = app
        .editor_doc()
        .and_then(|i| app.project.nodes.get(&i).map(|n| n.title.clone()))
        .unwrap_or_default();
    let area = centered(f.area(), 60, 16);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" Snapshots — {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [list_area, hint] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    if app.snaps.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "No snapshots yet.",
                Style::default().fg(DIM),
            )),
            list_area,
        );
    } else {
        let items: Vec<ListItem> = app
            .snaps
            .iter()
            .map(|n| ListItem::new(Line::from(Span::raw(crate::project::pretty_stamp(n)))))
            .collect();
        let list = List::new(items).highlight_style(
            Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default();
        state.select(Some(app.snap_sel.min(app.snaps.len() - 1)));
        f.render_stateful_widget(list, list_area, &mut state);
    }

    f.render_widget(
        Paragraph::new(Span::styled(
            if app.snap_confirm {
                "press d again to delete · any other key cancels"
            } else {
                "t take · enter restore · d delete · esc close"
            },
            Style::default().fg(if app.snap_confirm { Color::Red } else { DIM }),
        )),
        hint,
    );
}

fn draw_confirm(f: &mut Frame, app: &mut App) {
    let title = app
        .selected_id()
        .and_then(|i| app.project.nodes.get(&i).map(|n| n.title.clone()))
        .unwrap_or_default();
    let area = centered(f.area(), 60, 5);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red))
        .title(Span::styled(
            " Delete ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    let text = vec![
        Line::from(format!("Delete \"{title}\" and everything inside it?")),
        Line::from(""),
        Line::from(Span::styled(
            "y to confirm · any other key to cancel",
            Style::default().fg(DIM),
        )),
    ];
    f.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_help(f: &mut Frame) {
    let rows: &[(&str, &str)] = &[
        ("↑ ↓ / j k", "move through the binder"),
        ("→ ←", "expand / collapse, or step to parent"),
        ("space", "toggle folder"),
        ("enter", "open the document for writing"),
        ("esc", "leave the editor, back to the binder"),
        ("", ""),
        ("ctrl-b", "bold the selection, or the word at the cursor"),
        ("ctrl-i", "italic — or Tab, with text selected"),
        ("alt-c", "centre the current line"),
        ("alt-p", "insert a page break"),
        ("", ""),
        ("n / f", "new document / new folder"),
        ("r / s", "rename / edit synopsis"),
        ("t / l / w", "status / label / keywords"),
        ("i", "include or exclude from compile"),
        ("c", "compile just this subtree"),
        ("v", "snapshots of this document"),
        ("ctrl-f", "search: plain text, or /regex/"),
        ("", ""),
        ("click", "select a row, card, or place the cursor"),
        ("wheel", "scroll whichever pane is under the pointer"),
        ("d", "delete (asks first)"),
        ("alt + ↑ ↓", "reorder among siblings"),
        ("alt + → ←", "indent / outdent"),
        ("", ""),
        ("F2 / F3 / F4", "editor · cards · outline"),
        ("F6", "continuous mode (whole container as one flow)"),
        ("ctrl+↑ ↓", "step between documents in the flow"),
        ("F5", "compile to a single Markdown file"),
        ("F7", "mouse on/off (off restores drag-to-select)"),
        ("", ""),
        ("ctrl-s", "save"),
        ("ctrl-q", "save and quit"),
        ("F1", "this help"),
        ("", ""),
        ("", "press any key to close"),
    ];

    // Two columns, so the card stays short enough for a modest terminal.
    // Split at the section break nearest the middle.
    let mid = {
        let target = rows.len() / 2;
        rows.iter()
            .enumerate()
            .filter(|(_, (k, v))| k.is_empty() && v.is_empty())
            .min_by_key(|(i, _)| i.abs_diff(target))
            .map(|(i, _)| i)
            .unwrap_or(target)
    };
    let (left_rows, right_rows) = rows.split_at(mid);
    let right_rows = &right_rows[1..]; // drop the break we split on

    let render_row = |(k, v): &(&str, &str)| {
        Line::from(vec![
            Span::styled(
                format!("  {k:<12}"),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled((*v).to_string(), Style::default().fg(Color::Reset)),
        ])
    };

    let col_h = left_rows.len().max(right_rows.len()) as u16 + 2;
    let area = centered(f.area(), 108, col_h);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Jqln ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(inner);
    f.render_widget(
        Paragraph::new(left_rows.iter().map(render_row).collect::<Vec<_>>()),
        left,
    );
    f.render_widget(
        Paragraph::new(right_rows.iter().map(render_row).collect::<Vec<_>>()),
        right,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Project;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    /// Each test gets its own directory; these run in parallel and would
    /// otherwise race on creating and deleting the same path.
    fn scratch_app(tag: &str) -> App {
        let dir = std::env::temp_dir().join(format!("jqln-ui-{}-{}", std::process::id(), tag));
        let _ = std::fs::remove_dir_all(&dir);
        App::new(Project::create(&dir, "The Salt Road").unwrap())
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_binder_and_editor() {
        let mut app = scratch_app("binder");
        let out = render(&mut app, 90, 18);
        println!("{out}");
        assert!(out.contains("The Salt Road"), "project name in binder title");
        assert!(out.contains("Manuscript") && out.contains("Opening Scene"));
        assert!(out.contains("total"), "status bar word counts");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn renders_prose_with_soft_wrap() {
        let mut app = scratch_app("wrap");
        // Select "Opening Scene" and type a long paragraph.
        app.sel = 2;
        let id = app.editor_doc().expect("row 2 is a document");
        app.ensure_editor(&id);
        app.editors.get_mut(&id).unwrap().insert_str(
            "The road out of the salt flats was white and it went on for a very long way indeed.",
        );
        app.focus = Focus::Editor;
        let out = render(&mut app, 90, 14);
        println!("{out}");
        // The paragraph is longer than the editor pane, so it must occupy >1 row
        // and must not be truncated at the pane edge.
        assert!(out.contains("The road out of the salt flats"));
        assert!(out.contains("indeed."), "tail of the paragraph must wrap into view");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn continuous_mode_stitches_documents_into_one_flow() {
        use crate::project::Kind;
        let mut app = scratch_app("flow");
        // Add two more scenes beside the starter one.
        let chapter = {
            let id = app.rows()[1].0.clone();  // "Chapter One"
            id
        };
        let a = app.rows()[2].0.clone();       // "Opening Scene"
        let b = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
        let c = app.project.insert(&chapter, None, "Third Scene", Kind::Text);
        app.project.set_body(&a, "Alpha prose.".into());
        app.project.set_body(&b, "Beta prose.".into());
        app.project.set_body(&c, "Gamma prose.".into());

        app.sel = 2;              // select "Opening Scene"
        app.continuous = true;
        app.focus = Focus::Editor;

        let out = render(&mut app, 90, 20);
        println!("{out}");
        assert!(out.contains("continuous"), "pane should announce the mode");
        // All three documents appear together, each under its own title.
        for expected in ["Opening Scene", "Alpha prose.", "Second Scene", "Beta prose.", "Third Scene", "Gamma prose."] {
            assert!(out.contains(expected), "continuous flow missing {expected:?}");
        }
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn continuous_flow_scrolls_and_keeps_the_edited_document_visible() {
        use crate::project::Kind;
        let mut app = scratch_app("scroll");
        let chapter = app.rows()[1].0.clone();
        // Enough documents to overflow a short pane several times over.
        let mut ids = vec![app.rows()[2].0.clone()];
        for i in 0..12 {
            let id = app.project.insert(&chapter, None, &format!("Scene {i}"), Kind::Text);
            app.project.set_body(&id, format!("Body of scene {i}."));
            ids.push(id);
        }
        app.continuous = true;
        app.focus = Focus::Editor;

        // Land on the last document; the view must scroll down to reveal it.
        let last = ids.last().unwrap().clone();
        app.select_id(&last);
        let out = render(&mut app, 60, 10);
        println!("{out}");
        assert!(app.scroll > 0, "flow should have scrolled to reach the last document");
        assert!(out.contains("Body of scene 11."), "edited document must be on screen");
        assert!(!out.contains("Body of scene 0."), "far-off documents should be scrolled away");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn card_view_shows_synopses_and_navigates_the_grid() {
        use crate::project::Kind;
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        fn k(code: KeyCode) -> KeyEvent {
            KeyEvent { code, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
        }

        let mut app = scratch_app("cards");
        let chapter = app.rows()[1].0.clone();
        let first = app.rows()[2].0.clone();
        let second = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
        app.project.nodes.get_mut(&second).unwrap().synopsis = "They reach the coast.".into();

        app.sel = 1;                       // select the "Chapter One" folder
        app.on_key(k(KeyCode::F(3)));      // switch to cards
        // Selection dropped onto the first child rather than the container.
        assert_eq!(app.selected_id().as_deref(), Some(first.as_str()));

        let out = render(&mut app, 90, 16);
        println!("{out}");
        assert!(out.contains("cards"));
        assert!(out.contains("The one where it begins."), "starter synopsis");
        assert!(out.contains("They reach the coast."), "second card synopsis");

        // Right moves along the row of cards.
        app.on_key(k(KeyCode::Right));
        assert_eq!(app.selected_id().as_deref(), Some(second.as_str()));
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn card_grid_pages_instead_of_dropping_cards() {
        use crate::project::Kind;
        let mut app = scratch_app("paging");
        let chapter = app.rows()[1].0.clone();
        let mut ids = vec![app.rows()[2].0.clone()];
        for i in 0..11 {
            let id = app.project.insert(&chapter, None, &format!("Scene {i}"), Kind::Text);
            app.project.nodes.get_mut(&id).unwrap().synopsis = format!("Synopsis {i}");
            ids.push(id);
        }
        app.view = View::Corkboard;
        app.sel = 2;

        // A window this size fits two rows of cards, not six.
        let out = render(&mut app, 92, 20);
        assert!(out.contains("more row(s) below"), "should signal hidden rows");
        assert!(!out.contains("Synopsis 10"), "later cards are off this page");

        // Selecting a late card scrolls it into view rather than dropping it.
        let last = ids.last().unwrap().clone();
        app.select_id(&last);
        let out = render(&mut app, 92, 20);
        println!("{out}");
        assert!(app.card_scroll > 0, "grid should have scrolled");
        assert!(out.contains("Synopsis 10"), "selected card must be drawn");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn outline_lists_word_counts_and_compile_flags() {
        let mut app = scratch_app("outline");
        let scene = app.rows()[2].0.clone();
        app.ensure_editor(&scene);
        app.editors.get_mut(&scene).unwrap().insert_str("one two three four five");

        app.view = View::Outliner;
        let out = render(&mut app, 80, 12);
        println!("{out}");
        assert!(out.contains("outline"));
        assert!(out.contains("5 w"), "word count must reflect unsaved typing");
        // "Research" is excluded from compile in the starter project.
        assert!(out.contains("Research"));
        assert!(out.contains("·"), "excluded rows marked");
        assert!(out.contains("✓"), "included rows marked");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn renders_search_results_and_snapshot_list() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        fn k(code: KeyCode, m: KeyModifiers) -> KeyEvent {
            KeyEvent { code, modifiers: m, kind: KeyEventKind::Press, state: KeyEventState::NONE }
        }
        let mut app = scratch_app("find");
        app.sel = 2;
        let id = app.editor_doc().unwrap();
        app.ensure_editor(&id);
        app.editors.get_mut(&id).unwrap().insert_str("the white road\nand the salt");

        app.on_key(k(KeyCode::Char('f'), KeyModifiers::CONTROL));
        for c in "the".chars() {
            app.on_key(k(KeyCode::Char(c), KeyModifiers::NONE));
        }
        app.on_key(k(KeyCode::Enter, KeyModifiers::NONE));
        let out = render(&mut app, 90, 22);
        println!("{out}");
        assert!(out.contains("matches for"));
        assert!(out.contains("the white road"));
        assert!(out.contains("enter to jump"));
        app.on_key(k(KeyCode::Esc, KeyModifiers::NONE));

        app.on_key(k(KeyCode::Char('v'), KeyModifiers::NONE));
        app.on_key(k(KeyCode::Char('t'), KeyModifiers::NONE));
        let out = render(&mut app, 90, 22);
        println!("{out}");
        assert!(out.contains("Snapshots — Opening Scene"));
        assert!(out.contains("d delete"), "delete hint should be offered");
        // The snapshot is listed as a readable date, not a raw stamp.
        assert!(out.contains("-") && out.contains(":"));
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    fn click(app: &mut App, x: u16, y: u16) {
        use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
    }

    fn wheel(app: &mut App, x: u16, y: u16, down: bool) {
        use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
        app.on_mouse(MouseEvent {
            kind: if down { MouseEventKind::ScrollDown } else { MouseEventKind::ScrollUp },
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
    }

    #[test]
    fn clicking_the_tree_selects_that_row() {
        let mut app = scratch_app("mouse-tree");
        render(&mut app, 90, 16);   // establishes pane geometry
        assert_eq!(app.sel, 0);

        // Row 2 of the tree is "Opening Scene"; the pane starts below its border.
        let (bx, by) = (app.pane_binder.x + 4, app.pane_binder.y + 2);
        click(&mut app, bx, by);
        assert_eq!(app.sel, 2);
        let id = app.selected_id().unwrap();
        assert_eq!(app.project.nodes[&id].title, "Opening Scene");
        assert!(matches!(app.focus, Focus::Binder));
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn clicking_the_page_places_the_cursor() {
        let mut app = scratch_app("mouse-text");
        app.sel = 2;
        let id = app.editor_doc().unwrap();
        app.ensure_editor(&id);
        app.editors.get_mut(&id).unwrap().insert_str("first line\nsecond line\nthird line");
        render(&mut app, 90, 16);

        // Click into the middle of the second line of prose.
        let x = app.pane_editor.x + 3;
        let y = app.pane_editor.y + 1;
        click(&mut app, x, y);
        assert!(matches!(app.focus, Focus::Editor));
        assert_eq!(app.editors[&id].cursor(), (1, 3), "cursor should land where clicked");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn clicking_a_card_selects_it() {
        use crate::project::Kind;
        let mut app = scratch_app("mouse-card");
        let chapter = app.rows()[1].0.clone();
        let second = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
        app.view = View::Corkboard;
        app.sel = 2;
        render(&mut app, 92, 16);

        let (_, rect) = app
            .card_hits
            .iter()
            .find(|(id, _)| *id == second)
            .cloned()
            .expect("second card should have been drawn");
        click(&mut app, rect.x + 2, rect.y + 1);
        assert_eq!(app.selected_id().as_deref(), Some(second.as_str()));
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn the_wheel_scrolls_the_pane_under_the_pointer() {
        use crate::project::Kind;
        let mut app = scratch_app("mouse-wheel");
        let chapter = app.rows()[1].0.clone();
        for i in 0..10 {
            let id = app.project.insert(&chapter, None, &format!("Scene {i}"), Kind::Text);
            app.project.set_body(&id, format!("Body {i}."));
        }
        app.continuous = true;
        app.sel = 2;
        render(&mut app, 90, 14);

        // Wheeling over the tree moves the selection, not the flow.
        let before_scroll = app.scroll;
        let (bx, by) = (app.pane_binder.x + 2, app.pane_binder.y + 1);
        wheel(&mut app, bx, by, true);
        assert!(app.sel > 2, "wheel over the tree should move the selection");
        assert_eq!(app.scroll, before_scroll, "the flow should not have moved");

        // Wheeling over the flow scrolls it instead.
        app.sel = 2;
        render(&mut app, 90, 14);
        let before_sel = app.sel;
        let (fx, fy) = (app.flow_inner.x + 5, app.flow_inner.y + 2);
        wheel(&mut app, fx, fy, true);
        assert_eq!(app.sel, before_sel, "the selection should not have moved");
        assert!(app.scroll > 0, "wheel over the flow should scroll it");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn clicking_a_document_in_the_flow_focuses_it() {
        use crate::project::Kind;
        let mut app = scratch_app("mouse-flow");
        let chapter = app.rows()[1].0.clone();
        let second = app.project.insert(&chapter, None, "Second Scene", Kind::Text);
        app.project.set_body(&second, "Beta prose here.".into());
        app.continuous = true;
        app.sel = 2;
        render(&mut app, 90, 18);

        let (_, rect) = app
            .flow_hits
            .iter()
            .find(|(id, _)| *id == second)
            .cloned()
            .expect("second document should be visible in the flow");
        click(&mut app, rect.x + 5, rect.y);
        assert_eq!(app.selected_id().as_deref(), Some(second.as_str()));
        assert!(matches!(app.focus, Focus::Editor));
        // Offscreen-to-screen translation put the cursor on the clicked column.
        assert_eq!(app.editors[&second].cursor(), (0, 5));
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn the_lighthouse_appears_on_the_empty_pane_when_there_is_room() {
        let mut app = scratch_app("lighthouse");
        // Tall terminal: the art fits.
        let out = render(&mut app, 90, 30);
        println!("{out}");
        assert!(out.contains("O"), "lamp");
        assert!(out.contains("~~~~~~~~~~~~~"), "water line");
        assert!(out.contains("Manuscript"));

        // Short terminal: the art is dropped rather than clipped.
        let out = render(&mut app, 90, 12);
        assert!(!out.contains("~~~~~~~~~~~~~"), "art must not be drawn when cramped");
        assert!(out.contains("Manuscript"), "the useful text still shows");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn the_editor_styles_markup_in_place() {
        let mut app = scratch_app("markup");
        app.sel = 2;
        let id = app.editor_doc().unwrap();
        app.ensure_editor(&id);
        app.editors.get_mut(&id).unwrap().insert_str("say **now** ok");
        app.focus = Focus::Editor;

        let mut t = Terminal::new(TestBackend::new(90, 12)).unwrap();
        t.draw(|f| draw(f, &mut app)).unwrap();
        let buf = t.backend().buffer().clone();

        let area = app.pane_editor;
        let mut bold_runs: Vec<String> = Vec::new();
        let mut dim_star = false;
        for y in area.y..area.y + area.height {
            let mut run = String::new();
            for x in area.x..area.x + area.width {
                let cell = &buf[(x, y)];
                if cell.modifier.contains(Modifier::BOLD) {
                    run.push_str(cell.symbol());
                } else if !run.is_empty() {
                    bold_runs.push(std::mem::take(&mut run));
                }
                if cell.symbol() == "*" && cell.modifier.contains(Modifier::DIM) {
                    dim_star = true;
                }
            }
            if !run.is_empty() {
                bold_runs.push(std::mem::take(&mut run));
            }
        }
        assert!(bold_runs.iter().any(|r| r == "now"), "the word between ** ** must render bold, got {bold_runs:?}");
        assert!(dim_star, "the ** markers must be dimmed rather than shown at full weight");
        let _ = std::fs::remove_dir_all(&app.project.root);
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = scratch_app("help");
        app.modal = Modal::Help;
        let out = render(&mut app, 90, 24);
        println!("{out}");
        assert!(out.contains("Jqln") && out.contains("reorder among siblings"));
        let _ = std::fs::remove_dir_all(&app.project.root);
    }
}
