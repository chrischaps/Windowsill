//! The Win32 side of the glass: the extended styles winit has no setting for.
//!
//! winit owns the window's styles and rewrites `GWL_EXSTYLE` whenever one of its
//! own flags changes (visibility, z-order, cursor hit-testing). So click-through
//! goes through winit's `set_cursor_hittest(false)`, which keeps
//! `WS_EX_LAYERED | WS_EX_TRANSPARENT` set across those rewrites, and the two bits
//! winit doesn't know about are added here. Call [`make_glass`] again after any
//! winit call that might change the window's flags.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos,
    WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// The extended styles the sill must always have.
pub const GLASS_EX_STYLE: u32 = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;

/// The window's HWND, or `None` if winit didn't give us a Win32 handle.
pub fn hwnd(window: &Window) -> Option<HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as HWND),
        _ => None,
    }
}

/// Adds the glass styles to the window: clicks fall through to whatever is
/// underneath, it never takes focus, and it has no taskbar button or Alt+Tab entry.
pub fn make_glass(window: &Window) -> anyhow::Result<()> {
    let hwnd = hwnd(window).ok_or_else(|| anyhow::anyhow!("the sill has no Win32 window handle"))?;
    // SAFETY: `hwnd` is a live window owned by this thread's winit event loop.
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let glass = (ex_style | GLASS_EX_STYLE) & !WS_EX_APPWINDOW;
        if glass != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, glass as isize);
            // Style changes only take effect after a frame change.
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        // A layered window stays invisible until it is given an alpha. Fully opaque
        // here; per-pixel transparency is the job of the renderer (#4).
        if SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) == 0 {
            anyhow::bail!("SetLayeredWindowAttributes failed: {}", std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// The window's current extended style, for logging and checks.
pub fn ex_style(window: &Window) -> Option<u32> {
    let hwnd = hwnd(window)?;
    // SAFETY: reading a style from a live window has no side effects.
    Some(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32)
}
