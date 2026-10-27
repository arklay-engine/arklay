//! The wall-clock time in the user's time zone, for save-file names.
//!
//! Unix asks `localtime_r`, Windows `GetLocalTime`; elsewhere there is no
//! reading.

/// A local calendar date and time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    /// `1..=12`.
    pub month: u8,
    /// `1..=31`.
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

/// The current local time, or `None` when the platform cannot say.
#[cfg(unix)]
pub fn now() -> Option<LocalTime> {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    let seconds = libc::time_t::try_from(since.as_secs()).ok()?;
    // SAFETY: `tm` is plain data that `localtime_r` fills in; both pointers
    // are valid for the call.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&seconds, &mut tm).is_null() {
            return None;
        }
        tm
    };
    Some(LocalTime {
        year: tm.tm_year + 1900,
        month: (tm.tm_mon + 1) as u8,
        day: tm.tm_mday as u8,
        hour: tm.tm_hour as u8,
        minute: tm.tm_min as u8,
        second: tm.tm_sec.min(59) as u8,
        millisecond: since.subsec_millis() as u16,
    })
}

/// The current local time, or `None` when the platform cannot say.
#[cfg(windows)]
pub fn now() -> Option<LocalTime> {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    // SAFETY: `GetLocalTime` fills in the plain-data `SYSTEMTIME`.
    let time = unsafe {
        let mut time = std::mem::zeroed();
        GetLocalTime(&mut time);
        time
    };
    Some(LocalTime {
        year: i32::from(time.wYear),
        month: time.wMonth as u8,
        day: time.wDay as u8,
        hour: time.wHour as u8,
        minute: time.wMinute as u8,
        second: time.wSecond as u8,
        millisecond: time.wMilliseconds,
    })
}

/// The current local time, or `None` when the platform cannot say.
#[cfg(not(any(unix, windows)))]
pub fn now() -> Option<LocalTime> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_is_a_plausible_calendar_time() {
        let Some(time) = now() else { return };
        assert!(time.year >= 2024);
        assert!((1..=12).contains(&time.month));
        assert!((1..=31).contains(&time.day));
        assert!(time.hour < 24 && time.minute < 60 && time.second < 60);
        assert!(time.millisecond < 1000);
    }
}
