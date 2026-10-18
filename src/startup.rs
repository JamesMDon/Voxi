use windows::core::{w, Result, HRESULT, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::*;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
// Task Manager's Startup apps page disables entries here without touching the Run key.
const APPROVED_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const VALUE_NAME: PCWSTR = w!("Voxi");

/// On only when the Run entry launches this exe & Task Manager has not disabled it,
/// so moving Voxi to another folder shows the option as off until it is set again.
pub(crate) fn is_enabled() -> bool {
    let (Some(command), Some(executable)) = (read_run_command(), executable()) else {
        return false;
    };
    command
        .trim()
        .trim_matches('"')
        .eq_ignore_ascii_case(&executable)
        && !disabled_by_windows()
}

pub(crate) fn set_enabled(enabled: bool) -> Result<()> {
    unsafe {
        // Clearing the approval entry also re-enables a launch disabled in Task Manager.
        ignore_missing(RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            APPROVED_KEY,
            VALUE_NAME,
        ))?;
        if !enabled {
            return ignore_missing(RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME));
        }
        let executable = executable().ok_or_else(windows::core::Error::from_win32)?;
        let command: Vec<u16> = format!("\"{executable}\"")
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            (command.len() * size_of::<u16>()) as u32,
        )
    }
}

fn executable() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().into_owned())
}

fn read_run_command() -> Option<String> {
    unsafe {
        let mut size = 0u32;
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .ok()?;
        let mut buffer = vec![0u16; (size as usize).div_ceil(size_of::<u16>())];
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()?;
        let length = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..length]))
    }
}

fn disabled_by_windows() -> bool {
    let mut data = [0u8; 12];
    let mut size = data.len() as u32;
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APPROVED_KEY,
            VALUE_NAME,
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    // An odd first byte marks the entry as disabled; a missing entry means enabled.
    read.is_ok() && size > 0 && data[0] & 1 == 1
}

fn ignore_missing(result: Result<()>) -> Result<()> {
    match result {
        Err(error) if error.code() == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) => Ok(()),
        other => other,
    }
}
