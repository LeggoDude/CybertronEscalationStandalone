//! Puts the demo's icon (icon/foc_esc.ico, made by tools/make_icon.py) and its
//! name into foc_esc.exe when it is built for Windows. File Explorer shows the
//! icon for the program, and the game puts it on its window and the taskbar
//! (src/appicon.rs). The name, "Cybertron Escalation Standalone", is the
//! program's version information (its File description and Product name, which
//! File Explorer's Properties > Details lists): Task Manager names the program
//! by it, and NVIDIA's recording tools name the folder its videos and
//! screenshots go into after it (without it they took the name of the folder
//! foc_esc.exe is in: "release").
//!
//! Its version there is the installer's when tools/make_installer.py builds it
//! (FOC_ESC_VERSION: the date and time Setup was made, four numbers, as Setup is
//! named; the main menu shows the same at its bottom left), otherwise the
//! package's (Cargo.toml's).
//!
//! Both are put into a compiled resource file (.res) here, which the Windows
//! linker Rust uses (Visual Studio's link.exe) takes as it is, so no extra
//! crates or tools are needed. Without the icon the program is built with its
//! name alone; on another system, without either.

use std::env;
use std::fs;
use std::path::PathBuf;

/// The icon, in the project's folder.
const ICON: &str = "icon/foc_esc.ico";
/// The icon's resource number: the one src/appicon.rs loads (APP_ICON).
const ICON_ID: u16 = 1;
/// The program's name: its File description and Product name.
const NAME: &str = "Cybertron Escalation Standalone";

// Resource types (winuser.h).
const RT_ICON: u16 = 3;
const RT_GROUP_ICON: u16 = 14;
const RT_VERSION: u16 = 16;
/// The version information's resource number (VS_VERSION_INFO).
const VERSION_ID: u16 = 1;
/// US English, the resources' language as Windows' resource compilers write it.
const LANGUAGE: u16 = 0x0409;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed={ICON}");
    // (Its version is the installer's, or the package's.)
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-env-changed=FOC_ESC_VERSION");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!(
            "cargo::warning=the program's icon and name are only put into it when built with Rust's usual Windows (MSVC) toolchain"
        );
        return;
    }
    let dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let (version, text) = installer_version().unwrap_or_else(|| {
        let [major, minor, patch] = ["MAJOR", "MINOR", "PATCH"].map(|part| {
            env::var(format!("CARGO_PKG_VERSION_{part}"))
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
        });
        ([major, minor, patch, 0], format!("{major}.{minor}.{patch}"))
    });
    // (Every .res starts with an empty entry.)
    let mut res = Vec::new();
    resource(&mut res, 0, 0, 0, 0, &[]);
    if let Err(e) = fs::read(dir.join(ICON))
        .map_err(|e| e.to_string())
        .and_then(|ico| icon_res(&mut res, &ico))
    {
        println!("cargo::warning={ICON}: {e}; the program is built without its icon");
    }
    resource(
        &mut res,
        RT_VERSION,
        VERSION_ID,
        0x0030,
        LANGUAGE,
        &version_info(NAME, version, &text),
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("foc_esc.res");
    fs::write(&out, res).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    println!("cargo::rustc-link-arg-bins={}", out.display());
}

/// The .ico's resources, added to `res`: each of its pictures (one per size) as
/// an icon resource numbered from 1 up, and their list (the icon group,
/// ICON_ID) from which Windows picks the size it needs. Nothing is added if the
/// file isn't one.
fn icon_res(res: &mut Vec<u8>, ico: &[u8]) -> Result<(), String> {
    let u16_at = |at: usize| {
        ico.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
    };
    let u32_at = |at: usize| {
        ico.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if u16_at(0) != Some(0) || u16_at(2) != Some(1) {
        return Err("not an icon (.ico) file".into());
    }
    let count = u16_at(4).filter(|&n| n > 0).ok_or("no pictures in it")?;
    let mut icons = Vec::new();
    // The group: reserved, type (1: icons), count, then an entry per picture.
    let mut group = Vec::new();
    group.extend_from_slice(&[0, 0, 1, 0]);
    group.extend_from_slice(&count.to_le_bytes());
    for i in 0..usize::from(count) {
        let entry = 6 + 16 * i;
        let cut_short = || "its list of pictures is cut short".to_string();
        // Width, height, colour count, reserved, planes, bits per pixel.
        let head = ico.get(entry..entry + 8).ok_or_else(cut_short)?;
        let size = u32_at(entry + 8).ok_or_else(cut_short)?;
        let offset = u32_at(entry + 12).ok_or_else(cut_short)? as usize;
        let data = offset
            .checked_add(size as usize)
            .and_then(|end| ico.get(offset..end))
            .ok_or("a picture's data is past the file's end")?;
        let id = i as u16 + 1;
        resource(&mut icons, RT_ICON, id, 0x1010, LANGUAGE, data);
        let (mut planes, mut bits) = (
            u16::from_le_bytes([head[4], head[5]]),
            u16::from_le_bytes([head[6], head[7]]),
        );
        // (Some icon makers leave these 0: the picture's own header has them.)
        if planes == 0 || bits == 0 {
            (planes, bits) = picture_depth(data);
        }
        group.extend_from_slice(&head[..4]);
        group.extend_from_slice(&planes.to_le_bytes());
        group.extend_from_slice(&bits.to_le_bytes());
        group.extend_from_slice(&size.to_le_bytes());
        group.extend_from_slice(&id.to_le_bytes());
    }
    resource(&mut icons, RT_GROUP_ICON, ICON_ID, 0x1030, LANGUAGE, &group);
    res.extend_from_slice(&icons);
    Ok(())
}

/// The installer's version (FOC_ESC_VERSION, as make_installer.py gives it: up to
/// four numbers, each up to 65535, with dots between), if it gives one: as
/// numbers, and as it is written.
fn installer_version() -> Option<([u16; 4], String)> {
    let v = env::var("FOC_ESC_VERSION").ok()?;
    let v = v.trim();
    if v.is_empty() {
        return None;
    }
    let parts: Option<Vec<u16>> = v.split('.').map(|p| p.parse().ok()).collect();
    let Some(parts) = parts.filter(|p| p.len() <= 4) else {
        println!(
            "cargo::warning=FOC_ESC_VERSION {v:?} isn't a version (up to four numbers): the package's is used"
        );
        return None;
    };
    let mut out = [0; 4];
    out[..parts.len()].copy_from_slice(&parts);
    Some((out, v.to_string()))
}

/// The program's version information (a VS_VERSIONINFO): its version (major,
/// minor, patch, build) as numbers, and its strings in US English (Unicode):
/// `name` as its File description and Product name, its version (`text`), and
/// its file's name.
fn version_info(name: &str, [major, minor, patch, build]: [u16; 4], text: &str) -> Vec<u8> {
    // VS_FIXEDFILEINFO: its signature and layout version, the file's and the
    // product's version (each as two DWORDs: major.minor, patch.build), which
    // flags count (all) and are set (none), for 32-bit (and 64-bit) Windows, an
    // application, and no date.
    let ms = (u32::from(major) << 16) | u32::from(minor);
    let ls = (u32::from(patch) << 16) | u32::from(build);
    let fixed: Vec<u8> = [
        0xfeef_04bd,
        0x0001_0000,
        ms,
        ls,
        ms,
        ls,
        0x3f,
        0,
        0x0004_0004,
        1,
        0,
        0,
        0,
    ]
    .iter()
    .flat_map(|d: &u32| d.to_le_bytes())
    .collect();
    let strings: Vec<Vec<u8>> = [
        ("FileDescription", name),
        ("FileVersion", text),
        ("InternalName", "foc_esc"),
        ("OriginalFilename", "foc_esc.exe"),
        ("ProductName", name),
        ("ProductVersion", text),
    ]
    .iter()
    .map(|(key, value)| version_block(key, Some(Value::Text(value)), &[]))
    .collect();
    // (US English, 0x0409, in Unicode, code page 1200: 0x04b0.)
    let table = version_block("040904B0", None, &strings);
    let string_info = version_block("StringFileInfo", None, &[table]);
    let translation = version_block(
        "Translation",
        Some(Value::Binary(&[0x09, 0x04, 0xb0, 0x04])),
        &[],
    );
    let var_info = version_block("VarFileInfo", None, &[translation]);
    version_block(
        "VS_VERSION_INFO",
        Some(Value::Binary(&fixed)),
        &[string_info, var_info],
    )
}

/// A version information block's value: text (its length counted in UTF-16
/// characters, with its terminating 0) or binary (in bytes).
enum Value<'a> {
    Text(&'a str),
    Binary(&'a [u8]),
}

/// One block of version information: its length, its value's length and type
/// (1 for text), its key (UTF-16, ending in 0), its value and its children, each
/// starting on a 4-byte boundary. Its length leaves out any padding after it.
fn version_block(key: &str, value: Option<Value>, children: &[Vec<u8>]) -> Vec<u8> {
    let utf16 = |s: &str| -> Vec<u8> {
        s.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    };
    let (value_len, kind, value) = match value {
        Some(Value::Text(t)) => (t.encode_utf16().count() + 1, 1u16, utf16(t)),
        Some(Value::Binary(b)) => (b.len(), 0, b.to_vec()),
        None => (0, 1, Vec::new()),
    };
    let mut out = vec![0, 0];
    out.extend_from_slice(&(value_len as u16).to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&utf16(key));
    out.resize(out.len().next_multiple_of(4), 0);
    out.extend_from_slice(&value);
    for child in children {
        out.resize(out.len().next_multiple_of(4), 0);
        out.extend_from_slice(child);
    }
    let len = out.len() as u16;
    out[..2].copy_from_slice(&len.to_le_bytes());
    out
}

/// A picture's planes and bits per pixel: a PNG's are always 1 and 32; a
/// bitmap's are in its BITMAPINFOHEADER.
fn picture_depth(data: &[u8]) -> (u16, u16) {
    if data.starts_with(b"\x89PNG") {
        return (1, 32);
    }
    let at = |i: usize| {
        data.get(i..i + 2)
            .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
    };
    (at(12).max(1), at(14))
}

/// One resource: its header (with the type and number as ordinals), its data,
/// and padding to the next 4 bytes.
fn resource(res: &mut Vec<u8>, kind: u16, id: u16, memory_flags: u16, language: u16, data: &[u8]) {
    res.extend_from_slice(&(data.len() as u32).to_le_bytes());
    // The header's size.
    res.extend_from_slice(&32u32.to_le_bytes());
    res.extend_from_slice(&[0xff, 0xff]);
    res.extend_from_slice(&kind.to_le_bytes());
    res.extend_from_slice(&[0xff, 0xff]);
    res.extend_from_slice(&id.to_le_bytes());
    // Data version, memory flags, language, version, characteristics.
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&memory_flags.to_le_bytes());
    res.extend_from_slice(&language.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(&0u32.to_le_bytes());
    res.extend_from_slice(data);
    res.resize(res.len().next_multiple_of(4), 0);
}
