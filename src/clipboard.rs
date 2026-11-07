//! Pasting into the demo's text fields: the Join by IP Address prompt, the host's
//! and joiner's game passwords, Display Name and the Customize screen's rename
//! prompt. Ctrl+V (or Shift+Insert) puts the clipboard's text where the typing
//! goes: its first line (a copied address or password often ends in a line
//! break), taken as typing it would be (the same characters let in, up to the
//! field's length).
//!
//! The clipboard is read from Windows itself (user32's OpenClipboard and
//! GetClipboardData, its CF_UNICODETEXT), so nothing more is needed to build the
//! demo; elsewhere from the system's own paste command (pbpaste, wl-paste, xclip
//! or xsel), if it has one.

use bevy::input::ButtonInput;
use bevy::input::keyboard::{KeyCode, KeyboardInput};

/// Whether this frame's keys ask to paste: Ctrl+V or Shift+Insert. (Ctrl with
/// Alt is AltGr on many keyboards, which types characters of its own: not a
/// paste.)
pub fn paste_pressed(keys: &ButtonInput<KeyCode>) -> bool {
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    (ctrl_only(keys) && keys.just_pressed(KeyCode::KeyV))
        || (shift && keys.just_pressed(KeyCode::Insert))
}

/// Whether a key press is the paste shortcut's V, which isn't typed.
pub fn is_paste_key(key: &KeyboardInput, keys: &ButtonInput<KeyCode>) -> bool {
    key.key_code == KeyCode::KeyV && ctrl_only(keys)
}

fn ctrl_only(keys: &ButtonInput<KeyCode>) -> bool {
    keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && !keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight])
}

/// The clipboard's first line of text, if it holds text.
pub fn line() -> Option<String> {
    text().map(|t| first_line(&t).to_string())
}

/// A text's first line (without its line break).
pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

/// `pasted` added to `field` as typing it would be: the characters `keep` lets in,
/// while the field has fewer than `max`.
pub fn paste_into(field: &mut String, pasted: &str, max: usize, keep: impl Fn(&char) -> bool) {
    for c in pasted.chars().filter(keep) {
        if field.chars().count() >= max {
            break;
        }
        field.push(c);
    }
}

/// The clipboard's text.
pub fn text() -> Option<String> {
    platform::text()
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    /// The clipboard's Unicode text (UTF-16, ending in 0).
    const CF_UNICODETEXT: u32 = 13;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsClipboardFormatAvailable(format: u32) -> i32;
        fn OpenClipboard(owner: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalLock(mem: *mut c_void) -> *mut c_void;
        fn GlobalUnlock(mem: *mut c_void) -> i32;
        fn GlobalSize(mem: *mut c_void) -> usize;
    }

    pub fn text() -> Option<String> {
        // SAFETY: the clipboard is opened before its data is read and closed
        // after; the data's memory is locked while it is read, and read no
        // further than its size or its terminating 0.
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
                return None;
            }
            // (Another program may have it open for a moment.)
            let mut open = false;
            for _ in 0..5 {
                if OpenClipboard(null_mut()) != 0 {
                    open = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if !open {
                return None;
            }
            let mut out = None;
            let data = GetClipboardData(CF_UNICODETEXT);
            if !data.is_null() {
                let chars = GlobalLock(data) as *const u16;
                if !chars.is_null() {
                    let most = GlobalSize(data) / 2;
                    let mut n = 0;
                    while n < most && *chars.add(n) != 0 {
                        n += 1;
                    }
                    out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(
                        chars, n,
                    )));
                    GlobalUnlock(data);
                }
            }
            CloseClipboard();
            out
        }
    }
}

#[cfg(not(windows))]
mod platform {
    /// The text the system's paste command gives, the first that has one.
    pub fn text() -> Option<String> {
        let commands: &[(&str, &[&str])] = &[
            ("pbpaste", &[]),
            ("wl-paste", &["--no-newline"]),
            ("xclip", &["-selection", "clipboard", "-o"]),
            ("xsel", &["--clipboard", "--output"]),
        ];
        commands.iter().find_map(|(cmd, args)| {
            let out = std::process::Command::new(cmd).args(*args).output().ok()?;
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paste_is_its_first_line_typed_as_far_as_the_field_goes() {
        // A copied address with its line break, and spaces round it.
        let mut address = String::new();
        let pasted = first_line(" 192.168.1.20:27015 \r\nnext line");
        paste_into(&mut address, pasted.trim(), 64, char::is_ascii_graphic);
        assert_eq!(address, "192.168.1.20:27015");
        // A password: its spaces kept, what doesn't fit left out, typed on to
        // what is there.
        let mut password = String::from("ab");
        paste_into(&mut password, "c d\u{e9}efgh", 6, |c| {
            c.is_ascii_graphic() || *c == ' '
        });
        assert_eq!(password, "abc de");
        let mut name = String::new();
        paste_into(&mut name, first_line(""), 16, |_| true);
        assert!(name.is_empty());
    }

    #[test]
    fn ctrl_v_and_shift_insert_paste() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::ControlLeft);
        keys.press(KeyCode::KeyV);
        assert!(paste_pressed(&keys));
        // (AltGr: Ctrl and Alt.)
        keys.press(KeyCode::AltRight);
        assert!(!paste_pressed(&keys));
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyV);
        assert!(!paste_pressed(&keys));
        keys.press(KeyCode::ShiftRight);
        keys.press(KeyCode::Insert);
        assert!(paste_pressed(&keys));
    }
}
