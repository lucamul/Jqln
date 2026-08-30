//! All rendering. Nothing here mutates the project except to lazily open
//! editors for documents that are about to become visible.
//!
//! `draw` is the whole entry point; it picks a view, draws the status line,
//! then lays any modal on top. Each view and each overlay lives in its own
//! submodule.

mod binder;
mod cards;
mod editor;
mod modals;
mod outline;

#[cfg(test)]
mod tests;

use crate::app::{App, Modal, View};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(crate) const ACCENT: Color = Color::Cyan;
pub(crate) const DIM: Color = Color::DarkGray;

pub fn draw(f: &mut Frame, app: &mut App) {
    app.sync_cursor_modes();

    let [body, status] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());

    match app.view {
        View::Editor => draw_editor_view(f, app, body),
        View::Corkboard => cards::draw_cards(f, app, body),
        View::Outliner => outline::draw_outline(f, app, body),
    }
    draw_status(f, app, status);

    match app.modal {
        Modal::None => {}
        Modal::Help => modals::draw_help(f),
        Modal::ConfirmDelete => modals::draw_confirm(f, app),
        Modal::Input(p) => modals::draw_input(f, app, p),
        Modal::Results => modals::draw_results(f, app),
        Modal::Snapshots => modals::draw_snapshots(f, app),
        Modal::BookSettings => modals::draw_book_settings(f, app),
        Modal::Spell => modals::draw_spell(f, app),
        Modal::Notes => modals::draw_notes(f, app),
    }
}

fn draw_editor_view(f: &mut Frame, app: &mut App, area: Rect) {
    let binder_width = 34u16.min(area.width.saturating_sub(20)).max(20);
    let [left, right] =
        Layout::horizontal([Constraint::Length(binder_width), Constraint::Min(20)]).areas(area);
    binder::draw_binder(f, app, left);
    editor::draw_editor(f, app, right);
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

    if !app.spell_on {
        spans.push(Span::styled("  ·  ", Style::default().fg(DIM)));
        spans.push(Span::styled("spell off", Style::default().fg(DIM)));
    }

    if !app.status.is_empty() {
        spans.push(Span::styled("  ·  ", Style::default().fg(DIM)));
        spans.push(Span::styled(app.status.clone(), Style::default().fg(ACCENT)));
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Reset)),
        area,
    );
}
