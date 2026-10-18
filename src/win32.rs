//! The Win32 side of the glass: the extended styles winit has no setting for.
//!
//! winit owns the window's styles and rewrites `GWL_EXSTYLE` whenever one of its
//! own flags changes (visibility, z-order, cursor hit-testing). So click-through
//! goes through winit's `set_cursor_hittest(false)`, which keeps
//! `WS_EX_LAYERED | WS_EX_TRANSPARENT` set across those rewrites, and the two bits
//! winit doesn't know about are added here. Call [`make_glass`] again after any
//! winit call that might change the window's flags.
//!
//! winit also doesn't pass on the broadcasts that say the desktop has changed
//! shape, so [`watch_layout`] subclasses the window to hear them.

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETTASKBARPOS, APPBARDATA, DefSubclassProc,
    RemoveWindowSubclass, SHAppBarMessage, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, LWA_ALPHA, RegisterWindowMessageW, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetLayeredWindowAttributes,
    SetWindowLongPtrW, SetWindowPos, WM_DISPLAYCHANGE, WM_DPICHANGED, WM_NCDESTROY,
    WM_SETTINGCHANGE, WS_EX_APPWINDOW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT,
};
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::LayoutChanged;
use crate::placement::{Edge, Rect, Taskbar};

/// The extended styles the sill must always have.
pub const GLASS_EX_STYLE: u32 =
    WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;

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
    let hwnd =
        hwnd(window).ok_or_else(|| anyhow::anyhow!("the sill has no Win32 window handle"))?;
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
        // here: the per-pixel alpha comes from the glass's composition visual,
        // which this whole-window alpha leaves alone.
        if SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) == 0 {
            anyhow::bail!(
                "SetLayeredWindowAttributes failed: {}",
                std::io::Error::last_os_error()
            );
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

/// The taskbar's rectangle and edge from `SHAppBarMessage(ABM_GETTASKBARPOS)`,
/// in physical pixels (winit makes the process per-monitor DPI aware), or `None`
/// when the shell has no taskbar to report.
pub fn taskbar() -> Option<Taskbar> {
    // SAFETY: APPBARDATA is plain data, and all-zero is a valid starting value.
    let mut data: APPBARDATA = unsafe { std::mem::zeroed() };
    data.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
    // SAFETY: `data` is a correctly sized APPBARDATA that outlives the call.
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut data) } == 0 {
        return None;
    }
    let edge = match data.uEdge {
        ABE_LEFT => Edge::Left,
        ABE_TOP => Edge::Top,
        ABE_RIGHT => Edge::Right,
        ABE_BOTTOM => Edge::Bottom,
        _ => return None,
    };
    let rc = data.rc;
    Some(Taskbar {
        rect: Rect {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        },
        edge,
    })
}

/// Our id among the window's subclasses. Any value works if it's ours alone.
const LAYOUT_SUBCLASS: usize = 0x5111;

/// What the subclass needs to hand a layout change back to the event loop.
struct LayoutWatch {
    proxy: EventLoopProxy<LayoutChanged>,
    /// Explorer broadcasts this when it starts, so a restarted Explorer brings the sill back.
    taskbar_created: u32,
}

/// Wakes the event loop with a [`LayoutChanged`] whenever the window hears that
/// displays, DPI, the work area or the taskbar itself may have changed. The
/// window is otherwise untouched: every message still goes on to winit.
pub fn watch_layout(window: &Window, proxy: EventLoopProxy<LayoutChanged>) -> anyhow::Result<()> {
    let hwnd =
        hwnd(window).ok_or_else(|| anyhow::anyhow!("the sill has no Win32 window handle"))?;
    let name: Vec<u16> = "TaskbarCreated\0".encode_utf16().collect();
    // SAFETY: `name` is a NUL-terminated UTF-16 string that outlives the call.
    let taskbar_created = unsafe { RegisterWindowMessageW(name.as_ptr()) };
    let watch = Box::into_raw(Box::new(LayoutWatch {
        proxy,
        taskbar_created,
    }));
    // SAFETY: `hwnd` is live and owned by this thread. `watch` stays alive until
    // the subclass frees it on WM_NCDESTROY, the last message the window gets.
    if unsafe { SetWindowSubclass(hwnd, Some(layout_subclass), LAYOUT_SUBCLASS, watch as usize) }
        == 0
    {
        // SAFETY: the subclass was not installed, so nothing else holds `watch`.
        drop(unsafe { Box::from_raw(watch) });
        anyhow::bail!(
            "SetWindowSubclass failed: {}",
            std::io::Error::last_os_error()
        );
    }
    Ok(())
}

unsafe extern "system" fn layout_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    data: usize,
) -> LRESULT {
    let watch = data as *mut LayoutWatch;
    if msg == WM_NCDESTROY {
        // SAFETY: the window is going away, so this is the last time `watch` is
        // seen; it came from `Box::into_raw` in `watch_layout`.
        unsafe {
            RemoveWindowSubclass(hwnd, Some(layout_subclass), LAYOUT_SUBCLASS);
            drop(Box::from_raw(watch));
        }
    } else {
        // SAFETY: `watch` lives until WM_NCDESTROY, handled above.
        let watch = unsafe { &*watch };
        let reason = match msg {
            WM_DISPLAYCHANGE => Some("display change"),
            WM_DPICHANGED => Some("DPI change"),
            WM_SETTINGCHANGE => Some("settings change"),
            _ if msg == watch.taskbar_created => Some("taskbar created"),
            _ => None,
        };
        if let Some(reason) = reason {
            // Only fails once the loop has exited, when there is nothing to wake.
            let _ = watch.proxy.send_event(LayoutChanged(reason));
        }
    }
    // SAFETY: passes the message on to winit's own window procedure unchanged.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
