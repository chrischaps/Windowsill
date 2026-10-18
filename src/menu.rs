//! The tiny menu that opens over the candle: *Start at login*, *About* and *Quit*.
//!
//! It runs on a thread of its own. A popup menu spins its own message loop
//! until it closes, and on the sill's thread that loop would hand winit a
//! redraw while winit is still inside the handler that opened the menu. On its
//! own thread the sill keeps drawing underneath and never notices.
//!
//! The sill lets every click through and never takes focus, so the menu needs
//! a window of its own to belong to. That window is hidden and lives only as
//! long as the menu. The menu takes the foreground while it is open, which is
//! what lets a click anywhere else close it, and gives it back afterwards.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, DestroyWindow, GetForegroundWindow,
    GetWindowThreadProcessId, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING,
    PostMessageW, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_CENTERALIGN, TPM_NONOTIFY,
    TPM_RETURNCMD, TrackPopupMenu, WM_NULL, WS_EX_TOOLWINDOW, WS_POPUP,
};
use winit::event_loop::EventLoopProxy;

use crate::Wake;
use crate::hold::Choice;
use crate::login;

/// The items' ids. Zero is what the menu returns when nothing was chosen.
const START_AT_LOGIN: usize = 1;
const QUIT: usize = 2;

/// What *About* says: what this is, and what it does with what it hears.
const ABOUT: [&str; 4] = [
    concat!("Windowsill ", env!("CARGO_PKG_VERSION")),
    "A few things on the taskbar, in the light of the real sun and moon.",
    "It hears what this PC plays only to move with it.",
    "Nothing it hears is recorded, kept or sent.",
];

/// Opens the menu with its foot centred on `x`, `y` in physical screen pixels,
/// and wakes the event loop with a [`Wake::Menu`] once it has closed.
pub fn open(x: i32, y: i32, proxy: EventLoopProxy<Wake>) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("menu".into())
        .spawn(move || {
            let choice = show(x, y);
            // Only fails once the loop has exited, when there is nothing to tell.
            let _ = proxy.send_event(Wake::Menu(choice));
        })?;
    Ok(())
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Shows the menu and waits for it to close.
fn show(x: i32, y: i32) -> Choice {
    let class = wide("STATIC");
    // SAFETY: every handle made here is used and destroyed on this thread, and
    // every string outlives the call it is passed to (menus copy their text).
    unsafe {
        let owner = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        if owner.is_null() {
            tracing::warn!(
                "could not open the menu: {}",
                std::io::Error::last_os_error()
            );
            return Choice::Nothing;
        }
        let starts = login::enabled();
        let menu = build(starts);
        let before = GetForegroundWindow();
        take_foreground(owner, before);
        let picked = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_CENTERALIGN,
            x,
            y,
            0,
            owner,
            std::ptr::null(),
        ) as usize;
        // The documented nudge that lets the menu close properly next time.
        PostMessageW(owner, WM_NULL, 0, 0);
        // Closed with Escape or by choosing: the focus goes back where it was.
        // Closed by clicking another window: that window already has it.
        if GetForegroundWindow() == owner && !before.is_null() {
            SetForegroundWindow(before);
        }
        // Destroys the About submenu with it.
        DestroyMenu(menu);
        DestroyWindow(owner);
        match picked {
            START_AT_LOGIN => {
                match login::set(!starts) {
                    Ok(()) => tracing::info!("start at login: {}", !starts),
                    Err(err) => tracing::warn!("could not change start at login: {err}"),
                }
                Choice::Nothing
            }
            QUIT => Choice::Quit,
            _ => Choice::Nothing,
        }
    }
}

/// The menu's items, with *Start at login* ticked if it `starts`.
///
/// # Safety
/// Must be destroyed with `DestroyMenu` on the same thread.
unsafe fn build(starts: bool) -> HMENU {
    // SAFETY: as in `show`.
    unsafe {
        let about = CreatePopupMenu();
        for line in ABOUT {
            AppendMenuW(about, MF_STRING | MF_GRAYED, 0, wide(line).as_ptr());
        }
        let menu = CreatePopupMenu();
        AppendMenuW(
            menu,
            if starts {
                MF_STRING | MF_CHECKED
            } else {
                MF_STRING
            },
            START_AT_LOGIN,
            wide("Start at login").as_ptr(),
        );
        AppendMenuW(menu, MF_POPUP, about as usize, wide("About").as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, QUIT, wide("Quit").as_ptr());
        menu
    }
}

/// Brings `owner` to the foreground. Windows only lets the app the user is
/// using hand over the foreground, and the sill never hears a click, so for a
/// moment it shares that app's input to be allowed.
///
/// # Safety
/// `owner` must be a live window on this thread.
unsafe fn take_foreground(owner: HWND, before: HWND) {
    // SAFETY: attaching is undone before returning, and both threads exist
    // for the length of the call.
    unsafe {
        let ours = GetCurrentThreadId();
        let theirs = if before.is_null() {
            0
        } else {
            GetWindowThreadProcessId(before, std::ptr::null_mut())
        };
        let attached = theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, 1) != 0;
        if SetForegroundWindow(owner) == 0 {
            tracing::debug!(
                "the menu could not take the foreground; a click elsewhere may not close it"
            );
        }
        if attached {
            AttachThreadInput(ours, theirs, 0);
        }
    }
}
