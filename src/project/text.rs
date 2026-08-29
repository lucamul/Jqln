//! Small text helpers: timestamps without a date crate, word counting, and
//! turning a title into a filename-safe slug.

/// `YYYYMMDD-HHMMSS` in UTC, without pulling in a date library.
pub(crate) fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d, hh, mm, ss) = civil_from_epoch(secs);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// The current calendar year in UTC.
pub fn now_year() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_epoch(secs).0 as u32
}

/// Render a snapshot name back as something readable.
pub fn pretty_stamp(name: &str) -> String {
    let base = name.split('-').collect::<Vec<_>>();
    if base.len() < 2 || base[0].len() != 8 || base[1].len() != 6 {
        return name.to_string();
    }
    let (d, t) = (base[0], base[1]);
    format!(
        "{}-{}-{} {}:{}:{}",
        &d[0..4],
        &d[4..6],
        &d[6..8],
        &t[0..2],
        &t[2..4],
        &t[4..6]
    )
}

/// Days-from-civil, after Howard Hinnant's calendar algorithms.
pub(crate) fn civil_from_epoch(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hh, mm, ss) = ((rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, hh, mm, ss)
}

pub fn count_words(s: &str) -> usize {
    let plain = crate::markup::strip(s);
    plain.split_whitespace().filter(|w| w.chars().any(|c| c.is_alphanumeric())).count()
}

pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            for l in c.to_lowercase() {
                out.push(l);
            }
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let trimmed: String = trimmed.chars().take(40).collect();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_convert_from_epoch_correctly() {
        // Known instants, checked against the civil calendar.
        assert_eq!(civil_from_epoch(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(civil_from_epoch(1_000_000_000), (2001, 9, 9, 1, 46, 40));
        // A leap day, which is where naive date maths usually breaks.
        assert_eq!(civil_from_epoch(1_709_164_800), (2024, 2, 29, 0, 0, 0));
        assert_eq!(pretty_stamp("20240229-000000"), "2024-02-29 00:00:00");
        assert_eq!(pretty_stamp("nonsense"), "nonsense");
    }

    #[test]
    fn word_and_slug_helpers() {
        assert_eq!(count_words("one two  three\nfour"), 4);
        assert_eq!(count_words("--- ...   "), 0);
        // Formatting markup is not prose and must not pad the count.
        assert_eq!(count_words("a **bold** word\n\n\\newpage\n\n::: center\nend\n:::"), 4);
        assert_eq!(slugify("Chapter One: The Beginning!"), "chapter-one-the-beginning");
        assert_eq!(slugify("!!!"), "untitled");
    }
}
