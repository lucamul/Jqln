//! Getting selected text out of the editor and onto the system clipboard.
//!
//! Two routes, tried together so the copy lands wherever the environment allows:
//!
//!   * **OSC 52** — an escape sequence the terminal itself acts on. Works over
//!     SSH and in most modern terminals (kitty, iTerm2, WezTerm, Alacritty, and
//!     tmux with `set-clipboard on`), but not Apple's Terminal.app.
//!   * the **platform clipboard command** (`pbcopy`, `wl-copy` / `xclip`,
//!     `clip`), which covers the terminals that ignore OSC 52.
//!
//! Both are best-effort: a failure anywhere is swallowed, and the writer still
//! has the `F7` route (drop mouse capture, select with the terminal).

use std::io::Write;
use std::process::{Command, Stdio};

/// Put `text` on the system clipboard by whatever means are available.
pub fn copy(text: &str) {
    osc52(text);
    platform_tool(text);
}

/// `ESC ] 52 ; c ; <base64> BEL`. The sequence has a practical length ceiling;
/// past it, leave the job to the platform tool.
fn osc52(text: &str) {
    if text.len() > 72_000 {
        return;
    }
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

fn platform_tool(text: &str) {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["-b", "-i"]),
        ]
    };

    for (bin, args) in candidates {
        let Ok(mut child) = Command::new(bin)
            .args(*args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
        return;
    }
}

/// Standard base64 (RFC 4648), just enough for the OSC 52 payload.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18 & 63) as usize] as char);
        s.push(T[(n >> 12 & 63) as usize] as char);
        s.push(if c.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        s.push(if c.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // A multi-byte character survives as its UTF-8 bytes.
        assert_eq!(base64("café".as_bytes()), "Y2Fmw6k=");
    }
}
