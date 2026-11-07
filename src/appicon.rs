//! The demo's icon on its window and on the taskbar (Windows): the one build.rs
//! puts into foc_esc.exe (icon/foc_esc.ico), which File Explorer shows for the
//! program. A window only has an icon when it is given one, and the one Bevy
//! opens is given none, so once the game's window is open it is given the
//! program's, at the sizes Windows draws in the window's corner and on the
//! taskbar.

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, RawHandleWrapper};

/// Once the game's window is open (its native window made: Bevy adds its
/// RawHandleWrapper then), gives it the program's icon.
pub fn set_window_icon(opened: Query<&Window, (With<PrimaryWindow>, Added<RawHandleWrapper>)>) {
    for window in &opened {
        #[cfg(windows)]
        match win32::set_icon(window.name.as_deref().unwrap_or(win32::WINIT_CLASS)) {
            Ok(()) => info!("Window icon set"),
            Err(why) => warn!("The window's icon was not set: {why}"),
        }
        #[cfg(not(windows))]
        let _ = window;
    }
}

#[cfg(windows)]
mod win32 {
    //! The few Windows functions this needs (user32 and kernel32, which every
    //! Windows program has), declared here rather than adding a crate for them.

    use std::ffi::c_void;
    use std::ptr;

    type Handle = *mut c_void;

    /// The icon's resource number in the program (build.rs's ICON_ID).
    const APP_ICON: usize = 1;
    /// The window class winit gives a window that has no name of its own (Bevy
    /// uses a Window's name as its class).
    pub const WINIT_CLASS: &str = "Window Class";

    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    const IMAGE_ICON: u32 = 1;
    const SM_CXICON: i32 = 11;
    const SM_CYICON: i32 = 12;
    const SM_CXSMICON: i32 = 49;
    const SM_CYSMICON: i32 = 50;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(each: unsafe extern "system" fn(Handle, isize) -> i32, param: isize) -> i32;
        fn GetWindowThreadProcessId(window: Handle, process: *mut u32) -> u32;
        fn GetClassNameW(window: Handle, name: *mut u16, len: i32) -> i32;
        fn GetSystemMetrics(index: i32) -> i32;
        fn LoadImageW(
            module: Handle,
            name: *const u16,
            kind: u32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> Handle;
        fn PostMessageW(window: Handle, message: u32, wparam: usize, lparam: isize) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
        fn GetModuleHandleW(name: *const u16) -> Handle;
    }

    /// What `each_window` looks for, and the windows it finds.
    struct Search {
        process: u32,
        class: Vec<u16>,
        found: Vec<Handle>,
    }

    /// For EnumWindows: notes the window if it is one of this program's, of the
    /// class looked for.
    unsafe extern "system" fn each_window(window: Handle, search: isize) -> i32 {
        // SAFETY: `search` is the &mut Search set_icon passed EnumWindows, which
        // calls this before it returns.
        let search = unsafe { &mut *(search as *mut Search) };
        let mut process = 0;
        // SAFETY: a window EnumWindows gives, and a u32 to write to.
        unsafe { GetWindowThreadProcessId(window, &mut process) };
        if process == search.process {
            let mut class = [0u16; 256];
            // SAFETY: the buffer and its length.
            let len = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
            if len > 0 && class[..len as usize] == search.class[..] {
                search.found.push(window);
            }
        }
        // Go on to the next window.
        1
    }

    /// Gives this program's windows of that class its icon, big (the taskbar's
    /// and Alt+Tab's) and small (the window's corner). The messages are posted,
    /// not sent: the window's thread (Bevy's main one) takes them when it is
    /// free, so this does not wait on it from whichever thread this runs on.
    pub fn set_icon(class: &str) -> Result<(), &'static str> {
        let mut search = Search {
            // SAFETY: no arguments.
            process: unsafe { GetCurrentProcessId() },
            class: class.encode_utf16().collect(),
            found: Vec::new(),
        };
        // SAFETY: the callback and the Search it reads, alive for the call.
        unsafe { EnumWindows(each_window, &mut search as *mut Search as isize) };
        if search.found.is_empty() {
            return Err("its window was not found");
        }
        // SAFETY: a null name is this program's own module.
        let module = unsafe { GetModuleHandleW(ptr::null()) };
        for (which, cx, cy) in [
            (ICON_BIG, SM_CXICON, SM_CYICON),
            (ICON_SMALL, SM_CXSMICON, SM_CYSMICON),
        ] {
            // SAFETY: a resource number as its name (MAKEINTRESOURCE), and sizes.
            let icon = unsafe {
                LoadImageW(
                    module,
                    ptr::without_provenance(APP_ICON),
                    IMAGE_ICON,
                    GetSystemMetrics(cx),
                    GetSystemMetrics(cy),
                    0,
                )
            };
            if icon.is_null() {
                return Err("the program has no icon in it (it was built without build.rs's)");
            }
            for &window in &search.found {
                // SAFETY: a window of this program, and an icon kept for its life
                // (it is never destroyed).
                unsafe { PostMessageW(window, WM_SETICON, which, icon as isize) };
            }
        }
        Ok(())
    }
}
