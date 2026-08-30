//! The overlays: help, single-line input prompts, search results, the snapshot
//! list, and the delete confirmation. All are centred cards drawn on top of
//! whatever view is behind them.

use super::{ACCENT, DIM};
use crate::app::{App, BookField, Prompt};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

pub(super) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

pub(super) fn draw_input(f: &mut Frame, app: &mut App, prompt: Prompt) {
    let title = match prompt {
        Prompt::NewText => " New document ".to_string(),
        Prompt::NewFolder => " New folder ".to_string(),
        Prompt::Rename => " Rename ".to_string(),
        Prompt::Synopsis => " Synopsis ".to_string(),
        Prompt::Status => " Status ".to_string(),
        Prompt::Label => " Label ".to_string(),
        Prompt::Keywords => " Keywords (comma separated) ".to_string(),
        Prompt::Search => " Search ".to_string(),
        Prompt::ChapterHeading => " Heading — blank, 'title', or a name ".to_string(),
        Prompt::Book(field) => format!(" {} ", field.label()),
        Prompt::Comment => " Comment ".to_string(),
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

pub(super) fn draw_results(f: &mut Frame, app: &mut App) {
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

pub(super) fn draw_snapshots(f: &mut Frame, app: &mut App) {
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

pub(super) fn draw_book_settings(f: &mut Frame, app: &mut App) {
    let fields = BookField::ALL;
    let area = centered(f.area(), 66, fields.len() as u16 + 4);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Book settings ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [list_area, hint] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    let label_w = 20usize;
    let items: Vec<ListItem> = fields
        .iter()
        .map(|field| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  {:<label_w$}", field.label()),
                    Style::default().fg(DIM),
                ),
                Span::styled(field.display(&app.project.book), Style::default().fg(Color::Reset)),
            ]))
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(app.book_sel.min(fields.len() - 1)));
    f.render_stateful_widget(list, list_area, &mut state);

    f.render_widget(
        Paragraph::new(Span::styled(
            "enter edit · ←→ toggle / cycle · esc close · ctrl-s to save",
            Style::default().fg(DIM),
        )),
        hint,
    );
}

pub(super) fn draw_spell(f: &mut Frame, app: &mut App) {
    let area = centered(f.area(), 50, 14);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" “{}” ", app.spell_word),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [list_area, hint] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);

    if app.spell_suggestions.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "No suggestions.",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            )),
            list_area,
        );
    } else {
        let items: Vec<ListItem> = app
            .spell_suggestions
            .iter()
            .enumerate()
            .map(|(i, w)| {
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:>2}  ", i + 1), Style::default().fg(DIM)),
                    Span::styled(w.clone(), Style::default().fg(Color::Reset)),
                ]))
            })
            .collect();
        let list = List::new(items).highlight_style(
            Style::default().bg(ACCENT).fg(Color::Black).add_modifier(Modifier::BOLD),
        );
        let mut state = ListState::default();
        state.select(Some(app.spell_sel.min(app.spell_suggestions.len() - 1)));
        f.render_stateful_widget(list, list_area, &mut state);
    }

    f.render_widget(
        Paragraph::new(Span::styled(
            "enter / 1-9 to replace · a to add to dictionary · esc",
            Style::default().fg(DIM),
        )),
        hint,
    );
}

pub(super) fn draw_notes(f: &mut Frame, app: &mut App) {
    let title = app
        .notes_target
        .as_ref()
        .and_then(|id| app.project.nodes.get(id))
        .map(|n| n.title.clone())
        .unwrap_or_default();

    let area = centered(f.area(), 66, 18);
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" Notes — {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [edit_area, hint] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
    f.render_widget(&app.notes_input, edit_area);
    f.render_widget(
        Paragraph::new(Span::styled(
            "ctrl-s to save · esc to discard",
            Style::default().fg(DIM),
        )),
        hint,
    );
}

pub(super) fn draw_confirm(f: &mut Frame, app: &mut App) {
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

type Row = (&'static str, &'static str);

pub(super) fn draw_help(f: &mut Frame) {
    let rows: &[Row] = &[
        ("↑↓ / jk", "move through the binder"),
        ("→ ←", "expand / collapse / parent"),
        ("space", "fold a folder"),
        ("enter", "open for writing"),
        ("esc", "back to the binder"),
        ("", ""),
        ("ctrl-b", "bold selection / word"),
        ("ctrl-i", "italic (Tab if selected)"),
        ("ctrl-l", "centre line(s)"),
        ("ctrl-p", "page break"),
        ("ctrl-z", "undo (redo: ctrl-r)"),
        ("ctrl-g", "spell: fix word / toggle"),
        ("ctrl-n", "add / edit a comment"),
        ("ctrl-c", "copy selection to clipboard"),
        ("", ""),
        ("n / f", "new document / folder"),
        ("r / s", "rename / synopsis"),
        ("N", "edit notes"),
        ("t / l / w", "status / label / keywords"),
        ("h", "chapter heading (book)"),
        ("i / c", "compile: toggle / subtree"),
        ("v", "snapshots"),
        ("ctrl-f", "search (text or /regex/)"),
        ("d", "delete (asks first)"),
        ("K / J", "reorder up / down"),
        ("alt+↑↓", "reorder — also on cards"),
        ("alt+→←", "indent / outdent"),
        ("", ""),
        ("F2/F3/F4", "editor · cards · outline"),
        ("enter/bksp", "card: descend / go up"),
        ("click/drag", "place cursor / select text"),
        ("wheel", "scroll pane under pointer"),
        ("F6", "continuous mode"),
        ("ctrl+↑↓", "step between documents"),
        ("F5 / F8", "compile: .md file / PDF"),
        ("ctrl-b", "book / PDF settings"),
        ("F7", "mouse capture on / off"),
    ];

    let tail: &[Row] = &[
        ("", ""),
        ("ctrl-s/q", "save / save and quit"),
        ("", "press any key to close"),
    ];

    let mut all: Vec<Row> = rows.to_vec();
    #[cfg(feature = "assistant")]
    all.push(("F9", "assistant panel"));
    all.extend_from_slice(tail);
    let rows: &[Row] = &all;

    // One column when it fits vertically (keeps the blank-line grouping);
    // otherwise two columns of the non-blank rows split down the middle.
    const KEY_W: usize = 11;
    let desc_w = rows.iter().map(|(_, v)| v.chars().count()).max().unwrap_or(0);
    let col_w = (2 + KEY_W + 1 + desc_w) as u16;

    let one_col_fits = f.area().height >= rows.len() as u16 + 4;
    let two_col = !one_col_fits && f.area().width >= col_w * 2 + 3;

    let dense: Vec<Row>;
    let (left_rows, right_rows): (&[Row], &[Row]) = if two_col {
        dense = rows.iter().copied().filter(|(k, v)| !(k.is_empty() && v.is_empty())).collect();
        let mid = dense.len().div_ceil(2);
        (&dense[..mid], &dense[mid..])
    } else {
        (rows, &[])
    };

    let render_col = |slice: &[Row]| {
        slice
            .iter()
            .map(|(k, v)| {
                Line::from(vec![
                    Span::styled(
                        format!("  {k:<KEY_W$} "),
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled((*v).to_string(), Style::default().fg(Color::Reset)),
                ])
            })
            .collect::<Vec<_>>()
    };

    let body_h = left_rows.len().max(right_rows.len()) as u16 + 2;
    let want_w = if two_col { col_w * 2 + 3 } else { col_w + 2 };
    let area = centered(f.area(), want_w, body_h);
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

    if right_rows.is_empty() {
        f.render_widget(Paragraph::new(render_col(left_rows)), inner);
    } else {
        let [left, right] =
            Layout::horizontal([Constraint::Length(col_w), Constraint::Min(0)]).areas(inner);
        f.render_widget(Paragraph::new(render_col(left_rows)), left);
        f.render_widget(Paragraph::new(render_col(right_rows)), right);
    }
}
