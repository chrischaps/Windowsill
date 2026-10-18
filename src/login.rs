//! Starting at login, the way a real windowsill is there every morning.
//!
//! Windows starts whatever is named under `HKCU\…\Run` when the user signs
//! in. Task Manager can switch an entry off without removing it, by writing a
//! flag under `StartupApproved\Run`; the sill reads that too, so the menu's
//! tick says what will really happen at the next login.

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const NAME: &str = "Windowsill";

/// Whether this exe will start at the next login.
pub fn enabled() -> bool {
    let Some(command) = command() else {
        return false;
    };
    read_string(RUN, NAME).is_some_and(|there| there.eq_ignore_ascii_case(&command))
        && approved(read_bytes(APPROVED, NAME).as_deref())
}

/// Starts this exe at login from now on, or stops doing so.
pub fn set(on: bool) -> std::io::Result<()> {
    // Task Manager's flag goes either way: a fresh entry starts out approved.
    delete(APPROVED, NAME)?;
    if on {
        let command =
            command().ok_or_else(|| std::io::Error::other("could not tell where this exe is"))?;
        write_string(RUN, NAME, &command)
    } else {
        delete(RUN, NAME)
    }
}

/// How Windows should start this exe: quoted, and with none of the arguments
/// it was run with, so a `--day-in` test never becomes every morning.
fn command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(quoted(exe.to_str()?))
}

fn quoted(path: &str) -> String {
    format!("\"{path}\"")
}

/// Task Manager's flag: missing means approved, and an odd first byte means
/// the user switched it off.
fn approved(flag: Option<&[u8]>) -> bool {
    flag.and_then(|bytes| bytes.first())
        .is_none_or(|first| first & 1 == 0)
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn check(status: u32) -> std::io::Result<()> {
    match status {
        ERROR_SUCCESS => Ok(()),
        status => Err(std::io::Error::from_raw_os_error(status as i32)),
    }
}

fn read_string(key: &str, name: &str) -> Option<String> {
    let data = read(key, name, RRF_RT_REG_SZ)?;
    let units: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| u16::from_le_bytes(pair))
        .take_while(|&unit| unit != 0)
        .collect();
    String::from_utf16(&units).ok()
}

fn read_bytes(key: &str, name: &str) -> Option<Vec<u8>> {
    read(key, name, RRF_RT_REG_BINARY)
}

/// A value's raw bytes, or `None` if it is missing or of another type.
fn read(key: &str, name: &str, kind: u32) -> Option<Vec<u8>> {
    let (key, name) = (wide(key), wide(name));
    let mut size = 0u32;
    // SAFETY: the strings are NUL-terminated and outlive both calls, and the
    // second call writes at most `size` bytes into a buffer of that size.
    unsafe {
        let ask = |data: *mut u8, size: &mut u32| {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                std::ptr::null_mut(),
                data.cast(),
                size,
            )
        };
        if ask(std::ptr::null_mut(), &mut size) != ERROR_SUCCESS {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if ask(data.as_mut_ptr(), &mut size) != ERROR_SUCCESS {
            return None;
        }
        data.truncate(size as usize);
        Some(data)
    }
}

fn write_string(key: &str, name: &str, value: &str) -> std::io::Result<()> {
    let (key, name, value) = (wide(key), wide(name), wide(value));
    // SAFETY: the strings are NUL-terminated and outlive the call, and the
    // size given is the value's, terminator included.
    check(unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        )
    })
}

/// Removes a value, and is content if it was never there.
fn delete(key: &str, name: &str) -> std::io::Result<()> {
    let (key, name) = (wide(key), wide(name));
    // SAFETY: the strings are NUL-terminated and outlive the call.
    match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } {
        ERROR_FILE_NOT_FOUND => Ok(()),
        status => check(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::System::Registry::RegDeleteKeyW;

    /// A key of its own, so the tests never touch what starts at login.
    const SCRATCH: &str = r"Software\Windowsill-tests";

    #[test]
    fn the_command_is_quoted_and_bare() {
        assert_eq!(
            quoted(r"C:\Program Files\Windowsill\windowsill.exe"),
            r#""C:\Program Files\Windowsill\windowsill.exe""#
        );
    }

    #[test]
    fn task_managers_flag_is_read_by_its_first_byte() {
        assert!(approved(None));
        assert!(approved(Some(&[])));
        assert!(approved(Some(&[2, 0, 0, 0])));
        assert!(approved(Some(&[6, 0, 0, 0])));
        assert!(!approved(Some(&[3, 0, 0, 0])));
        assert!(!approved(Some(&[7, 0, 0, 0])));
    }

    #[test]
    fn a_value_written_reads_back_and_deletes_twice() {
        let name = "round-trip";
        write_string(SCRATCH, name, r#""C:\a b\ü.exe""#).unwrap();
        assert_eq!(
            read_string(SCRATCH, name).as_deref(),
            Some(r#""C:\a b\ü.exe""#)
        );
        assert_eq!(read_bytes(SCRATCH, name), None, "not a binary value");
        delete(SCRATCH, name).unwrap();
        delete(SCRATCH, name).unwrap();
        assert_eq!(read_string(SCRATCH, name), None);
        let key = wide(SCRATCH);
        // SAFETY: the string is NUL-terminated and outlives the call.
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, key.as_ptr()) };
    }
}
