//! The assistant pane: a header, the scrollable transcript, and the input line.

use super::{ACCENT, DIM};
use crate::app::App;
use crate::assistant::Role;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub(super) fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.assistant.focused;
    let border = if focused { ACCENT } else { DIM };

    let title = if app.assistant.busy { " assistant · …thinking " } else { " assistant " };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(Span::styled(
            title,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let [head, transcript, prompt] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .areas(inner);

    f.render_widget(
        Paragraph::new(Span::styled(
            crate::app::assistant_header(app),
            Style::default().fg(DIM),
        )),
        head,
    );

    // Transcript: flatten every turn into styled, wrapped lines, then show the
    // window ending `scroll_back` rows above the bottom.
    let width = transcript.width.max(1) as usize;
    let mut lines: Vec<Line> = Vec::new();
    for turn in &app.assistant.turns {
        let (tag, style) = match turn.role {
            Role::User => ("you", Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Role::Assistant => ("ai", Style::default().fg(Color::Reset)),
            Role::Local => ("·", Style::default().fg(DIM).add_modifier(Modifier::ITALIC)),
        };
        if turn.role != Role::Local {
            lines.push(Line::from(Span::styled(tag, style.add_modifier(Modifier::BOLD))));
        }
        for raw in turn.text.split('\n') {
            for chunk in wrap(raw, width) {
                let s = if turn.role == Role::Local {
                    Style::default().fg(DIM).add_modifier(Modifier::ITALIC)
                } else {
                    Style::default().fg(Color::Reset)
                };
                lines.push(Line::from(Span::styled(chunk, s)));
            }
        }
        lines.push(Line::from(""));
    }

    let h = transcript.height as usize;
    let total = lines.len();
    let max_back = total.saturating_sub(h);
    let back = (app.assistant.scroll_back as usize).min(max_back);
    let start = total.saturating_sub(h + back);
    let view: Vec<Line> = lines.into_iter().skip(start).take(h).collect();
    f.render_widget(Paragraph::new(view), transcript);

    // Prompt.
    let pblock = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            if app.assistant.busy { " ctrl-c cancels " } else { " enter sends · /help " },
            Style::default().fg(DIM),
        ));
    let pinner = pblock.inner(prompt);
    f.render_widget(pblock, prompt);
    f.render_widget(&app.assistant.input, pinner);
}

/// The "paste your API key" popup. The field is masked to its last four chars.
pub(super) fn draw_key_prompt(f: &mut Frame, app: &App) {
    let provider = &app.project.assistant.provider;
    let area = super::modals::centered(f.area(), 64, 6);
    f.render_widget(ratatui::widgets::Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {provider} API key "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let raw = app.input.lines().first().cloned().unwrap_or_default();
    let n = raw.chars().count();
    let masked = if n <= 4 {
        "•".repeat(n)
    } else {
        let tail: String = raw.chars().skip(n - 4).collect();
        format!("{}{tail}", "•".repeat(n - 4))
    };

    let lines = vec![
        Line::from(Span::styled(
            "Paste it and press Enter. Saved to ~/.config/jqln/config.toml (chmod 600).",
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            "Your messages and chosen context will then be sent to this provider.",
            Style::default().fg(DIM),
        )),
        Line::from(""),
        Line::from(Span::styled(
            if masked.is_empty() { "▏".to_string() } else { format!("{masked}▏") },
            Style::default().fg(Color::Reset),
        )),
    ];
    f.render_widget(Paragraph::new(lines), inner);
}

/// Break `s` into pieces no wider than `width` columns, on spaces where it can.
fn wrap(s: &str, width: usize) -> Vec<String> {
    if s.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    let mut line = String::new();
    for word in s.split(' ') {
        if line.is_empty() {
            line = word.to_string();
        } else if line.chars().count() + 1 + word.chars().count() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        }
        while line.chars().count() > width {
            let cut: String = line.chars().take(width).collect();
            out.push(cut);
            line = line.chars().skip(width).collect();
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}
