//! `jqln --install-dict <lang>` — fetch a Hunspell dictionary into
//! `~/.config/jqln/dictionaries/`.
//!
//! Dictionaries come from the community
//! [`wooorm/dictionaries`](https://github.com/wooorm/dictionaries) collection.
//! Most are GPL / LGPL / MPL — copyleft licences Jqln cannot bundle, but which
//! you are free to download and use. The licence text is saved alongside the
//! dictionary as `<lang>.LICENSE.txt`.

use std::path::Path;
use std::process::Command;

const BASE: &str = "https://raw.githubusercontent.com/wooorm/dictionaries/main/dictionaries";

/// A handful of common languages, for `--install-dict` with no argument and for
/// nicer messages. Any other `wooorm/dictionaries` code also works.
pub const KNOWN: &[(&str, &str)] = &[
    ("it", "Italian"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("pt", "Portuguese (Brazil)"),
    ("pt-PT", "Portuguese (Portugal)"),
    ("nl", "Dutch"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("nb", "Norwegian Bokmål"),
    ("pl", "Polish"),
    ("ru", "Russian"),
    ("cs", "Czech"),
    ("ro", "Romanian"),
    ("el", "Greek"),
    ("uk", "Ukrainian"),
    ("ca", "Catalan"),
];

/// The `--install-dict` (no argument) listing.
pub fn languages() -> String {
    let mut s = String::from("usage: jqln --install-dict <lang>\n\ncommon codes:\n");
    for (code, name) in KNOWN {
        s.push_str(&format!("  {code:<6} {name}\n"));
    }
    s.push_str("\n(`en` is built in. Any other wooorm/dictionaries code also works.)\n");
    s
}

/// Download the `<lang>` dictionary. Returns a message to print on success.
pub fn run(lang: &str) -> Result<String, String> {
    let lang = lang.trim();
    if lang.is_empty() || lang == "en" {
        return Err(languages());
    }

    let dir = crate::config::dictionaries_dir()
        .ok_or("no config directory (set $HOME or $XDG_CONFIG_HOME)")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    let fetcher = pick_fetcher().ok_or(
        "need `curl` or `wget` on PATH to download — or drop the .aff / .dic \
         files into the dictionaries folder yourself",
    )?;

    for (remote, local) in [
        ("index.aff", format!("{lang}.aff")),
        ("index.dic", format!("{lang}.dic")),
        ("license", format!("{lang}.LICENSE.txt")),
    ] {
        let url = format!("{BASE}/{lang}/{remote}");
        let dest = dir.join(&local);
        // The licence file is a bonus — do not fail the whole install for it.
        if let Err(e) = fetch(fetcher, &url, &dest) {
            if remote == "license" {
                eprintln!("jqln: (couldn't fetch the licence file: {e})");
            } else {
                return Err(format!(
                    "download failed for {lang}: {e}\n\
                     is \"{lang}\" a valid code? see github.com/wooorm/dictionaries/tree/main/dictionaries"
                ));
            }
        }
    }

    let name = KNOWN.iter().find(|(c, _)| *c == lang).map(|(_, n)| *n).unwrap_or(lang);
    let licence = std::fs::read_to_string(dir.join(format!("{lang}.LICENSE.txt")))
        .ok()
        .map(|t| {
            let head: String = t.lines().take(6).collect::<Vec<_>>().join("\n");
            format!("\n\nLicence ({lang}.LICENSE.txt):\n{head}\n…")
        })
        .unwrap_or_default();

    Ok(format!(
        "{name} dictionary installed to {}\n\
         Set `language = \"{lang}\"` in a project's [spelling] table (or press \
         L in the tree) to use it.{licence}",
        dir.display()
    ))
}

fn pick_fetcher() -> Option<&'static str> {
    ["curl", "wget"].into_iter().find(|bin| {
        Command::new(bin).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    })
}

fn fetch(bin: &str, url: &str, dest: &Path) -> Result<(), String> {
    let tmp = dest.with_extension("part");
    let status = match bin {
        "curl" => Command::new("curl").args(["-fsSL", url, "-o"]).arg(&tmp).status(),
        _ => Command::new("wget").args(["-q", url, "-O"]).arg(&tmp).status(),
    }
    .map_err(|e| e.to_string())?;
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("{bin} exited with {status}"));
    }
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_language_list_covers_the_common_codes() {
        let list = languages();
        assert!(list.contains("it") && list.contains("Italian"));
        assert!(list.contains("de") && list.contains("fr"));
        // `run` with no real language falls back to the same list.
        assert_eq!(run("").unwrap_err(), list);
        assert_eq!(run("en").unwrap_err(), list);
    }
}
