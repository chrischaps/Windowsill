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
//! shape, that the machine has woken from sleep, or that the screen has been
//! locked, so [`watch_layout`] subclasses the window to hear them.

use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows_sys::Win32::System::Time::{
    GetTimeZoneInformation, TIME_ZONE_ID_INVALID, TIME_ZONE_INFORMATION,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_LBUTTON, VK_MBUTTON, VK_RBUTTON,
};
use windows_sys::Win32::UI::Shell::{
    ABE_BOTTOM, ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETSTATE, ABM_GETTASKBARPOS, ABS_AUTOHIDE,
    APPBARDATA, DefSubclassProc, QUNS_BUSY, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
    RemoveWindowSubclass, SHAppBarMessage, SHQueryUserNotificationState, SetWindowSubclass,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetWindowLongPtrW, LWA_ALPHA, PBT_APMRESUMEAUTOMATIC,
    RegisterWindowMessageW, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetLayeredWindowAttributes, SetWindowLongPtrW, SetWindowPos, WM_DISPLAYCHANGE, WM_DPICHANGED,
    WM_NCDESTROY, WM_POWERBROADCAST, WM_SETTINGCHANGE, WM_WTSSESSION_CHANGE, WS_EX_APPWINDOW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
};
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::Wake;
use crate::hold::Pointer;
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
/// in physical pixels (winit makes the process per-monitor DPI aware), and
/// whether `ABM_GETSTATE` says it auto-hides, or `None` when the shell has no
/// taskbar to report.
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
    // SAFETY: as above; ABM_GETSTATE only reads `cbSize`.
    let state = unsafe { SHAppBarMessage(ABM_GETSTATE, &mut data) } as u32;
    Some(Taskbar {
        rect: Rect {
            left: rc.left,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        },
        edge,
        auto_hide: state & ABS_AUTOHIDE != 0,
    })
}

/// What has come in front of the sill, if anything: a fullscreen app, a game,
/// or a presentation. `None` also when the shell won't say, so the sill stays.
pub fn covered() -> Option<&'static str> {
    let mut state = 0;
    // SAFETY: `state` is a QUERY_USER_NOTIFICATION_STATE that outlives the call.
    if unsafe { SHQueryUserNotificationState(&mut state) } < 0 {
        return None;
    }
    match state {
        QUNS_BUSY => Some("a fullscreen app"),
        QUNS_RUNNING_D3D_FULL_SCREEN => Some("a fullscreen game"),
        QUNS_PRESENTATION_MODE => Some("a presentation"),
        _ => None,
    }
}

/// Where the mouse is and whether a button is down, or `None` when Windows
/// won't say, as on the secure desktop.
pub fn pointer() -> Option<Pointer> {
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` is a POINT that outlives the call.
    if unsafe { GetCursorPos(&mut at) } == 0 {
        return None;
    }
    // The physical buttons, whichever way round they are swapped.
    let pressed = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
        .into_iter()
        // SAFETY: reading a key's state has no side effects.
        .any(|key| unsafe { GetAsyncKeyState(i32::from(key)) } < 0);
    Some(Pointer {
        x: at.x,
        y: at.y,
        pressed,
    })
}

/// What `GetTimeZoneInformation` returns while daylight saving time is on.
const TIME_ZONE_ID_DAYLIGHT: u32 = 2;

/// How far the local clock is ahead of UTC right now, in seconds, or `None`
/// if Windows won't say.
pub fn utc_offset() -> Option<i64> {
    // SAFETY: TIME_ZONE_INFORMATION is plain data, and all-zero is a valid starting value.
    let mut zone: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `zone` is a TIME_ZONE_INFORMATION that outlives the call.
    let id = unsafe { GetTimeZoneInformation(&mut zone) };
    if id == TIME_ZONE_ID_INVALID {
        return None;
    }
    // The bias is what to add to local time to get UTC, in minutes.
    let bias = zone.Bias
        + if id == TIME_ZONE_ID_DAYLIGHT {
            zone.DaylightBias
        } else {
            zone.StandardBias
        };
    Some(-i64::from(bias) * 60)
}

/// How much CPU time the whole process has used so far, on every thread and in
/// the kernel as well as in user mode, or `None` if Windows won't say.
pub fn cpu_time() -> Option<std::time::Duration> {
    let mut times = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    let [created, exited, kernel, user] = &mut times;
    // SAFETY: the pseudo-handle needs no closing, and each FILETIME outlives the call.
    let ok = unsafe { GetProcessTimes(GetCurrentProcess(), created, exited, kernel, user) };
    if ok == 0 {
        return None;
    }
    // In units of 100 ns.
    let ticks = |t: &FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Some(std::time::Duration::from_nanos(
        (ticks(kernel) + ticks(user)) * 100,
    ))
}

/// Our id among the window's subclasses. Any value works if it's ours alone.
const LAYOUT_SUBCLASS: usize = 0x5111;

/// What the subclass needs to hand a layout change back to the event loop.
struct LayoutWatch {
    proxy: EventLoopProxy<Wake>,
    /// Explorer broadcasts this when it starts, so a restarted Explorer brings the sill back.
    taskbar_created: u32,
}

/// Wakes the event loop with a [`Wake::Layout`] whenever the window hears that
/// displays, DPI, the work area or the taskbar itself may have changed, with a
/// [`Wake::Woke`] when the machine wakes from sleep, and with [`Wake::Locked`]
/// and [`Wake::Unlocked`] as the screen locks and unlocks. The window is
/// otherwise untouched: every message still goes on to winit.
pub fn watch_layout(window: &Window, proxy: EventLoopProxy<Wake>) -> anyhow::Result<()> {
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
    // Without it the sill still runs; it just keeps drawing behind the lock screen.
    // SAFETY: `hwnd` is live; the subclass unregisters it on WM_NCDESTROY.
    if unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) } == 0 {
        tracing::warn!(
            "not watching for the screen to lock: {}",
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
            WTSUnRegisterSessionNotification(hwnd);
            RemoveWindowSubclass(hwnd, Some(layout_subclass), LAYOUT_SUBCLASS);
            drop(Box::from_raw(watch));
        }
    } else {
        // SAFETY: `watch` lives until WM_NCDESTROY, handled above.
        let watch = unsafe { &*watch };
        let wake = match msg {
            WM_DISPLAYCHANGE => Some(Wake::Layout("display change")),
            WM_DPICHANGED => Some(Wake::Layout("DPI change")),
            WM_SETTINGCHANGE => Some(Wake::Layout("settings change")),
            WM_POWERBROADCAST if wparam as u32 == PBT_APMRESUMEAUTOMATIC => Some(Wake::Woke),
            WM_WTSSESSION_CHANGE if wparam as u32 == WTS_SESSION_LOCK => Some(Wake::Locked),
            WM_WTSSESSION_CHANGE if wparam as u32 == WTS_SESSION_UNLOCK => Some(Wake::Unlocked),
            _ if msg == watch.taskbar_created => Some(Wake::Layout("taskbar created")),
            _ => None,
        };
        if let Some(wake) = wake {
            // Only fails once the loop has exited, when there is nothing to wake.
            let _ = watch.proxy.send_event(wake);
        }
    }
    // SAFETY: passes the message on to winit's own window procedure unchanged.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}
