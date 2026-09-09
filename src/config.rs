//! Jqln's per-user configuration directory — `~/.config/jqln` (or
//! `$XDG_CONFIG_HOME/jqln`, `%APPDATA%\jqln` on Windows). It holds nothing a
//! project needs; today just downloaded spell-check dictionaries.

use std::path::PathBuf;

/// The `jqln` config directory, if a home / config location can be found.
pub fn config_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("jqln"))
    }
    #[cfg(not(windows))]
    {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(x).join("jqln"));
        }
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("jqln"))
    }
}

/// Where downloaded Hunspell dictionaries live: `<config>/dictionaries`.
pub fn dictionaries_dir() -> Option<PathBuf> {
    config_dir().map(|d| d.join("dictionaries"))
}
