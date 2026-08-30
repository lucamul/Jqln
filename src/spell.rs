//! English spell checking.
//!
//! A bundled `en_US` Hunspell dictionary (SCOWL, permissively licensed — see
//! `assets/en_US.LICENSE.txt`) checked with the pure-Rust `spellbook` crate.
//! Misspelled words in the editor get a red underline; the writer's own
//! additions live in the `[spell]` table of `jqln.toml`.

use crate::markup::Highlight;
use ratatui::style::{Color, Modifier, Style};

/// Priority for the misspelling underline: above the faded markup markers,
/// below the editor's own selection and search layers.
const SPELL_PRIORITY: u8 = 3;

pub struct Spell {
    dict: spellbook::Dictionary,
}

impl Spell {
    /// Load the bundled dictionary plus the writer's own words.
    pub fn english(personal: &[String]) -> Self {
        let aff = include_str!("../assets/en_US.aff");
        let dic = include_str!("../assets/en_US.dic");
        let mut dict =
            spellbook::Dictionary::new(aff, dic).expect("the bundled en_US dictionary parses");
        for w in personal {
            let _ = dict.add(w);
        }
        Spell { dict }
    }

    /// Add a word to the running dictionary (does not touch `jqln.toml` — the
    /// caller records it in the project's personal list).
    pub fn learn(&mut self, word: &str) {
        let _ = self.dict.add(word);
    }

    /// Is `word` spelled correctly? Tolerates a capitalised sentence-opener and
    /// a curly apostrophe.
    pub fn is_correct(&self, word: &str) -> bool {
        if self.dict.check(word) {
            return true;
        }
        let lowered = word.to_lowercase();
        if lowered != word && self.dict.check(&lowered) {
            return true;
        }
        if word.contains('\u{2019}') {
            let straight = word.replace('\u{2019}', "'");
            if self.dict.check(&straight) {
                return true;
            }
        }
        false
    }

    /// Up to eight corrections for a misspelled word, best first.
    pub fn suggestions(&self, word: &str) -> Vec<String> {
        let mut out = Vec::new();
        self.dict.suggest(word, &mut out);
        out.truncate(8);
        out
    }

    /// A red-underline highlight for every misspelled word in `lines`.
    pub fn highlights(&self, lines: &[String]) -> Vec<Highlight> {
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
        Spell::english(&["Eldoria".to_string()])
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
