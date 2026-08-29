//! The book-settings screen: an in-editor view of the `[book]` table from
//! `jqln.toml`, reached with Ctrl-B from the tree. Each row edits one field
//! through the same single-line prompt the rest of the app uses; toggles and
//! the trim size cycle in place with the arrow keys.

use super::{App, Modal, Prompt};
use crate::project::Book;
use crossterm::event::{KeyCode, KeyEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BookField {
    Title,
    Subtitle,
    Author,
    CopyrightYear,
    CopyrightHolder,
    Publisher,
    Rights,
    Dedication,
    Trim,
    BodyFont,
    BodySize,
    ChapterLabel,
    SceneBreak,
    RunningHeads,
    ChaptersOnRecto,
    FrontMatterFolder,
}

enum Kind {
    Text,
    Number,
    Bool,
    Choice(&'static [&'static str]),
}

const TRIMS: &[&str] = &["5x8", "5.25x8", "5.5x8.5", "6x9", "a5"];

impl BookField {
    pub const ALL: [BookField; 16] = [
        BookField::Title,
        BookField::Subtitle,
        BookField::Author,
        BookField::CopyrightYear,
        BookField::CopyrightHolder,
        BookField::Publisher,
        BookField::Rights,
        BookField::Dedication,
        BookField::Trim,
        BookField::BodyFont,
        BookField::BodySize,
        BookField::ChapterLabel,
        BookField::SceneBreak,
        BookField::RunningHeads,
        BookField::ChaptersOnRecto,
        BookField::FrontMatterFolder,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BookField::Title => "Title",
            BookField::Subtitle => "Subtitle",
            BookField::Author => "Author",
            BookField::CopyrightYear => "Copyright year",
            BookField::CopyrightHolder => "Copyright holder",
            BookField::Publisher => "Publisher",
            BookField::Rights => "Rights line",
            BookField::Dedication => "Dedication",
            BookField::Trim => "Trim size",
            BookField::BodyFont => "Body font",
            BookField::BodySize => "Body size (pt)",
            BookField::ChapterLabel => "Chapter label",
            BookField::SceneBreak => "Scene break",
            BookField::RunningHeads => "Running heads",
            BookField::ChaptersOnRecto => "Chapters on recto",
            BookField::FrontMatterFolder => "Front-matter folder",
        }
    }

    fn kind(self) -> Kind {
        match self {
            BookField::CopyrightYear | BookField::BodySize => Kind::Number,
            BookField::RunningHeads | BookField::ChaptersOnRecto => Kind::Bool,
            BookField::Trim => Kind::Choice(TRIMS),
            _ => Kind::Text,
        }
    }

    /// The current value, formatted for the list. Empty text shows a hint.
    pub fn display(self, b: &Book) -> String {
        match self {
            BookField::Title if b.title.trim().is_empty() => "— (project name)".into(),
            BookField::CopyrightYear if b.copyright_year == 0 => "— (year compiled)".into(),
            BookField::CopyrightHolder if b.copyright_holder.trim().is_empty() => {
                "— (author)".into()
            }
            BookField::FrontMatterFolder if b.front_matter_folder.trim().is_empty() => {
                "Front Matter".into()
            }
            BookField::RunningHeads => yes_no(b.running_heads),
            BookField::ChaptersOnRecto => yes_no(b.chapters_on_recto),
            _ => {
                let v = self.raw(b);
                if v.is_empty() { "—".into() } else { v }
            }
        }
    }

    /// The current value as an editable string.
    fn raw(self, b: &Book) -> String {
        match self {
            BookField::Title => b.title.clone(),
            BookField::Subtitle => b.subtitle.clone(),
            BookField::Author => b.author.clone(),
            BookField::CopyrightYear => {
                if b.copyright_year == 0 { String::new() } else { b.copyright_year.to_string() }
            }
            BookField::CopyrightHolder => b.copyright_holder.clone(),
            BookField::Publisher => b.publisher.clone(),
            BookField::Rights => b.rights.clone(),
            BookField::Dedication => b.dedication.clone(),
            BookField::Trim => b.trim.clone(),
            BookField::BodyFont => b.body_font.clone(),
            BookField::BodySize => trim_float(b.body_size),
            BookField::ChapterLabel => b.chapter_label.clone(),
            BookField::SceneBreak => b.scene_break.clone(),
            BookField::FrontMatterFolder => b.front_matter_folder.clone(),
            BookField::RunningHeads => yes_no(b.running_heads),
            BookField::ChaptersOnRecto => yes_no(b.chapters_on_recto),
        }
    }

    fn set(self, b: &mut Book, text: &str) {
        let t = text.trim();
        match self {
            BookField::Title => b.title = t.into(),
            BookField::Subtitle => b.subtitle = t.into(),
            BookField::Author => b.author = t.into(),
            BookField::CopyrightYear => b.copyright_year = t.parse().unwrap_or(0),
            BookField::CopyrightHolder => b.copyright_holder = t.into(),
            BookField::Publisher => b.publisher = t.into(),
            BookField::Rights => b.rights = t.into(),
            BookField::Dedication => b.dedication = t.into(),
            BookField::Trim => {
                if !t.is_empty() {
                    b.trim = t.into();
                }
            }
            BookField::BodyFont => {
                if !t.is_empty() {
                    b.body_font = t.into();
                }
            }
            BookField::BodySize => {
                if let Ok(v) = t.parse::<f32>()
                    && (6.0..=24.0).contains(&v)
                {
                    b.body_size = v;
                }
            }
            BookField::ChapterLabel => b.chapter_label = t.into(),
            BookField::SceneBreak => b.scene_break = t.into(),
            BookField::FrontMatterFolder => b.front_matter_folder = t.into(),
            BookField::RunningHeads | BookField::ChaptersOnRecto => {}
        }
    }

    fn toggle_or_cycle(self, b: &mut Book, forward: bool) -> bool {
        match self.kind() {
            Kind::Bool => {
                match self {
                    BookField::RunningHeads => b.running_heads = !b.running_heads,
                    BookField::ChaptersOnRecto => b.chapters_on_recto = !b.chapters_on_recto,
                    _ => {}
                }
                true
            }
            Kind::Choice(opts) => {
                let cur = opts.iter().position(|o| *o == b.trim).unwrap_or(0);
                let next = if forward {
                    (cur + 1) % opts.len()
                } else {
                    (cur + opts.len() - 1) % opts.len()
                };
                b.trim = opts[next].to_string();
                true
            }
            _ => false,
        }
    }
}

fn yes_no(v: bool) -> String {
    if v { "yes".into() } else { "no".into() }
}

fn trim_float(v: f32) -> String {
    if (v - v.round()).abs() < f32::EPSILON {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}

impl App {
    pub(super) fn open_book_settings(&mut self) {
        self.book_sel = 0;
        self.modal = Modal::BookSettings;
    }

    pub(super) fn book_settings_key(&mut self, key: KeyEvent) {
        let last = BookField::ALL.len() - 1;
        let field = BookField::ALL[self.book_sel.min(last)];
        match key.code {
            KeyCode::Esc => self.modal = Modal::None,
            KeyCode::Up | KeyCode::Char('k') => {
                self.book_sel = self.book_sel.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.book_sel = (self.book_sel + 1).min(last);
            }
            KeyCode::Home => self.book_sel = 0,
            KeyCode::End => self.book_sel = last,
            KeyCode::Left | KeyCode::Right => {
                let forward = key.code == KeyCode::Right;
                if field.toggle_or_cycle(&mut self.project.book, forward) {
                    self.dirty = true;
                }
            }
            KeyCode::Char(' ') => {
                if field.toggle_or_cycle(&mut self.project.book, true) {
                    self.dirty = true;
                }
            }
            KeyCode::Enter => {
                if field.toggle_or_cycle(&mut self.project.book, true) {
                    self.dirty = true;
                } else {
                    let cur = field.raw(&self.project.book);
                    self.begin(Prompt::Book(field), &cur);
                }
            }
            _ => {}
        }
    }

    pub(super) fn commit_book_field(&mut self, field: BookField, text: String) {
        field.set(&mut self.project.book, &text);
        self.dirty = true;
        self.modal = Modal::BookSettings;
    }
}
