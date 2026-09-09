//! Spell checking against a Hunspell dictionary, via the pure-Rust `spellbook`
//! crate. Misspelled words in the editor get a red underline; the writer's own
//! additions live in the `[spelling]` table of `jqln.toml`.
//!
//! `en` ships in the binary (SCOWL `en_US`, permissively licensed — see
//! `assets/en_US.LICENSE.txt`). Other languages are Hunspell `.aff` / `.dic`
//! pairs the writer places in — or `jqln --install-dict <lang>` downloads to —
//! `~/.config/jqln/dictionaries/`.

pub mod install;

use crate::markup::Highlight;
use ratatui::style::{Color, Modifier, Style};

/// Priority for the misspelling underline: above the faded markup markers,
/// below the editor's own selection and search layers.
const SPELL_PRIORITY: u8 = 3;

pub struct Spell {
    /// `None` when no dictionary is loaded for the chosen language — the
    /// checker then flags nothing.
    dict: Option<spellbook::Dictionary>,
    pub lang: String,
    /// A sentence for the status line: absent when a dictionary is loaded.
    pub problem: Option<String>,
}

impl Spell {
    /// Load the dictionary for `lang` plus the writer's own words. Never fails:
    /// a missing or broken dictionary yields an inert checker and a `problem`
    /// message.
    pub fn load(lang: &str, personal: &[String]) -> Self {
        let lang = if lang.trim().is_empty() { "en" } else { lang.trim() };
        let (dict, problem) = match read_dictionary(lang) {
            Ok((aff, dic)) => match spellbook::Dictionary::new(&aff, &dic) {
                Ok(mut d) => {
                    for w in personal {
                        let _ = d.add(w);
                    }
                    (Some(d), None)
                }
                Err(e) => (None, Some(format!("{lang} dictionary is unreadable: {e}"))),
            },
            Err(problem) => (None, Some(problem)),
        };
        Spell { dict, lang: lang.to_string(), problem }
    }

    /// Whether a dictionary is actually loaded.
    pub fn ready(&self) -> bool {
        self.dict.is_some()
    }

    /// Add a word to the running dictionary (does not touch `jqln.toml` — the
    /// caller records it in the project's personal list).
    pub fn learn(&mut self, word: &str) {
        if let Some(d) = &mut self.dict {
            let _ = d.add(word);
        }
    }

    /// Is `word` spelled correctly? Tolerates a capitalised sentence-opener and
    /// a curly apostrophe. With no dictionary, everything is "correct".
    pub fn is_correct(&self, word: &str) -> bool {
        let Some(dict) = &self.dict else { return true };
        if dict.check(word) {
            return true;
        }
        let lowered = word.to_lowercase();
        if lowered != word && dict.check(&lowered) {
            return true;
        }
        if word.contains('\u{2019}') {
            let straight = word.replace('\u{2019}', "'");
            if dict.check(&straight) {
                return true;
            }
        }
        false
    }

    /// Up to eight corrections for a misspelled word, best first.
    pub fn suggestions(&self, word: &str) -> Vec<String> {
        let Some(dict) = &self.dict else { return Vec::new() };
        let mut out = Vec::new();
        dict.suggest(word, &mut out);
        out.truncate(8);
        out
    }

    /// A red-underline highlight for every misspelled word in `lines`.
    pub fn highlights(&self, lines: &[String]) -> Vec<Highlight> {
        if self.dict.is_none() {
            return Vec::new();
        }
        let style = Style::default().fg(Color::Red).add_modifier(Modifier::UNDERLINED);
        let mut out = Vec::new();
        for (row, line) in lines.iter().enumerate() {
            let t = line.trim();
            if t == crate::markup::PAGE_BREAK
                || t == crate::markup::CENTER_OPEN
                || t == crate::markup::CENTER_CLOSE
            {
                continue;
            }
            for (start, end, word) in words(line) {
                if !skip(&word) && !self.is_correct(&word) {
                    out.push((((row, start), (row, end)), style, SPELL_PRIORITY));
                }
            }
        }
        out
    }
}

/// The `.aff` and `.dic` text for `lang`. `en` is embedded; every other
/// language is read from `~/.config/jqln/dictionaries/`.
fn read_dictionary(lang: &str) -> Result<(String, String), String> {
    if lang == "en" {
        return Ok((
            include_str!("../assets/en_US.aff").to_string(),
            include_str!("../assets/en_US.dic").to_string(),
        ));
    }
    let dir = crate::config::dictionaries_dir()
        .ok_or("no config directory (set $HOME or $XDG_CONFIG_HOME)")?;
    let aff = dir.join(format!("{lang}.aff"));
    let dic = dir.join(format!("{lang}.dic"));
    if !aff.exists() || !dic.exists() {
        return Err(format!("no {lang} dictionary — run: jqln --install-dict {lang}"));
    }
    let read = |p: &std::path::Path| std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    Ok((read(&aff)?, read(&dic)?))
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || c == '\'' || c == '\u{2019}'
}

/// `(byte_start, byte_end, text)` for each run of word characters on the line.
fn words(line: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        if is_word_char(c) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            out.push((s, i, line[s..i].to_string()));
        }
    }
    if let Some(s) = start {
        out.push((s, line.len(), line[s..].to_string()));
    }
    out
}

/// The word around character column `col` in `line`, as `(start, end, text)` in
/// character columns. For the corrections popup.
pub fn word_at(line: &str, col: usize) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = line.chars().collect();
    // The cursor sits on a character; that character must be part of a word.
    // (At end of line, fall back to the character just behind it.)
    let anchor = if col < chars.len() && is_word_char(chars[col]) {
        col
    } else if col > 0 && col >= chars.len() && is_word_char(chars[col - 1]) {
        col - 1
    } else {
        return None;
    };
    let mut start = anchor;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = anchor + 1;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }
    Some((start, end, chars[start..end].iter().collect()))
}

/// Words the checker should leave alone: single letters, ALL-CAPS acronyms, or
/// words with an internal capital (brand names, code identifiers).
fn skip(word: &str) -> bool {
    let core = word.trim_matches(|c| c == '\'' || c == '\u{2019}');
    if core.chars().take(2).count() < 2 {
        return true;
    }
    if core.chars().filter(|c| c.is_alphabetic()).all(char::is_uppercase) {
        return true;
    }
    core.chars().skip(1).any(char::is_uppercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spell() -> Spell {
        Spell::load("en", &["Eldoria".to_string()])
    }

    #[test]
    fn an_uninstalled_language_yields_an_inert_checker() {
        let s = Spell::load("zz-not-a-lang", &[]);
        assert!(!s.ready());
        assert!(s.problem.is_some());
        assert!(s.is_correct("anything"), "nothing is flagged without a dictionary");
        assert!(s.highlights(&["total nonsense qwxz".to_string()]).is_empty());
    }

    #[test]
    fn flags_misspellings_and_respects_the_personal_list() {
        let s = spell();
        assert!(s.is_correct("receive"));
        assert!(s.is_correct("The")); // capitalised opener
        assert!(!s.is_correct("recieve"));
        assert!(s.is_correct("Eldoria"), "a learned name is accepted");
        assert!(!s.is_correct("Eldorian"));
    }

    #[test]
    fn skips_acronyms_and_short_words() {
        assert!(skip("a"));
        assert!(skip("NASA"));
        assert!(skip("iPhone"));
        assert!(!skip("cromulent"));
    }

    #[test]
    fn highlights_only_the_wrong_words() {
        let s = spell();
        let lines = vec!["the qwik brown fox".to_string(), "\\newpage".to_string()];
        let hl = s.highlights(&lines);
        assert_eq!(hl.len(), 1);
        let (((r, c0), (_, c1)), _, p) = hl[0];
        assert_eq!(r, 0);
        assert_eq!(&"the qwik brown fox"[c0..c1], "qwik");
        assert_eq!(p, SPELL_PRIORITY);
    }

    #[test]
    fn suggestions_lead_with_the_obvious_fix() {
        assert_eq!(spell().suggestions("teh").first().map(String::as_str), Some("the"));
    }

    #[test]
    fn word_at_finds_the_word_under_the_cursor() {
        assert_eq!(word_at("the qwik fox", 6), Some((4, 8, "qwik".to_string())));
        assert_eq!(word_at("the qwik fox", 3), None); // on the space
    }
}

