//! The launcher/engine log and the Windows console wire-up.
//!
//! The binary is a Windows GUI-subsystem program so double-clicking it never
//! flashes a console behind the launcher. [`init`] attaches to a parent
//! console when one exists (so `arklay pack list` still prints in a terminal,
//! and the test harness's pipes keep working); when there is neither a
//! console nor inherited pipes it points stdout and stderr at
//! `<primary data>/logs/arklay.log`, which also catches the game's
//! `eprintln!` output. The file rotates once it passes one megabyte.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Rotate the log once it grows past this many bytes.
const MAX_LOG_BYTES: u64 = 1024 * 1024;

static LOG_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

/// Prepare the log and, on Windows, the standard handles. Safe to call once
/// at the start of `main`; failures are ignored.
pub fn init() {
    let log = prepare();
    let _ = LOG_PATH.set(log.clone());
    if let Some(path) = &log {
        let header = format!(
            "--- arklay {} ({}), data root {} ---",
            crate::build_info::label(),
            unix_time(),
            crate::paths::primary_data_dir().display()
        );
        append(path, &header);
        append(path, "");
        append(
            path,
            &format!("config: {}", crate::paths::config_path().display()),
        );
        for root in crate::paths::data_roots() {
            append(path, &format!("data:   {}", root.display()));
        }
        append(path, "");
    }
    hook_std_handles(log.as_deref());
}

/// The log file, once [`init`] has run.
pub fn path() -> Option<&'static Path> {
    LOG_PATH.get().and_then(|path| path.as_deref())
}

/// Append one line to the log. Nothing happens before [`init`] or when the
/// log could not be created.
pub fn write(message: &str) {
    let Some(Some(path)) = LOG_PATH.get() else {
        return;
    };
    let _guard = LOCK.lock();
    append(path, message);
}

/// Create the log directory and rotate an oversized log.
fn prepare() -> Option<PathBuf> {
    let dir = crate::paths::log_dir();
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    let path = dir.join("arklay.log");
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > MAX_LOG_BYTES) {
        let old = dir.join("arklay.log.1");
        let _ = fs::remove_file(&old);
        let _ = fs::rename(&path, &old);
    }
    Some(path)
}

/// Append one line, ignoring every failure: logging must never take the
/// launcher down.
fn append(path: &Path, message: &str) {
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{message}");
}

/// The engine warnings [`warning!`](crate::warning) has printed: how many,
/// and the most recent with their numbers (counting from 1).
struct Warnings {
    count: u64,
    recent: std::collections::VecDeque<(u64, String)>,
}

impl Warnings {
    const fn new() -> Self {
        Self {
            count: 0,
            recent: std::collections::VecDeque::new(),
        }
    }
}

/// How many warnings [`Warnings::recent`] keeps.
const RECENT_WARNINGS: usize = 32;

/// The process's warnings.
#[cfg(not(test))]
static WARNINGS: Mutex<Warnings> = Mutex::new(Warnings::new());

// A test's warnings are its own thread's. The test harness runs every test
// of the crate in one process, each on a thread of its own, so with the
// process's count a strict script driver in one test would fail on a warning
// another test raised meanwhile.
#[cfg(test)]
thread_local! {
    static WARNINGS: std::cell::RefCell<Warnings> =
        const { std::cell::RefCell::new(Warnings::new()) };
}

/// Run `f` on the warnings [`warning!`](crate::warning) counts into.
#[cfg(not(test))]
fn with_warnings<R>(f: impl FnOnce(&mut Warnings) -> R) -> R {
    // A panic under the lock leaves the count as it was: go on counting.
    let mut warnings = WARNINGS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    f(&mut warnings)
}

/// Run `f` on the warnings [`warning!`](crate::warning) counts into.
#[cfg(test)]
fn with_warnings<R>(f: impl FnOnce(&mut Warnings) -> R) -> R {
    WARNINGS.with_borrow_mut(f)
}

/// Print an engine warning to stderr and count it.
///
/// Takes the same arguments as `eprintln!` and prints the same line. The
/// count is what a strict `arklay script` run checks after each engine call:
/// a warning raised while the script drives the game fails the script
/// instead of scrolling past (see [`warnings_since`]).
#[macro_export]
macro_rules! warning {
    ($($arg:tt)*) => {
        $crate::logging::note_warning(::std::format!($($arg)*))
    };
}

/// [`warning!`](crate::warning)'s body: print `message` and count it.
pub fn note_warning(message: String) {
    eprintln!("{message}");
    with_warnings(|warnings| {
        warnings.count += 1;
        if warnings.recent.len() == RECENT_WARNINGS {
            warnings.recent.pop_front();
        }
        warnings.recent.push_back((warnings.count, message));
    });
}

/// The number of warnings printed so far, and the text of the first one
/// after the `seen`th if it is still remembered.
pub fn warnings_since(seen: u64) -> (u64, Option<String>) {
    with_warnings(|warnings| {
        let first = warnings
            .recent
            .iter()
            .find(|(number, _)| *number > seen)
            .map(|(_, message)| message.clone());
        (warnings.count, first)
    })
}

/// Seconds since the Unix epoch, for the session header.
fn unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(windows)]
fn hook_std_handles(log: Option<&Path>) {
    use std::ptr;
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_ALWAYS,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };

    fn missing(handle: HANDLE) -> bool {
        handle.is_null() || handle == INVALID_HANDLE_VALUE
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    // A pipe or an inherited handle already works; leave it alone.
    let out = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let err = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    let out_missing = missing(out);
    let err_missing = missing(err);
    if !out_missing && !err_missing {
        return;
    }

    // A GUI process started from a terminal has no console of its own;
    // borrow the parent's and open CONOUT$ for the missing handles.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0 {
        let name = wide("CONOUT$");
        let console = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                ptr::null_mut(),
            )
        };
        if console != INVALID_HANDLE_VALUE {
            if out_missing {
                unsafe { SetStdHandle(STD_OUTPUT_HANDLE, console) };
            }
            if err_missing {
                unsafe { SetStdHandle(STD_ERROR_HANDLE, console) };
            }
            return;
        }
    }

    let Some(path) = log else {
        return;
    };
    let name = wide(&path.to_string_lossy());
    let file = unsafe {
        CreateFileW(
            name.as_ptr(),
            windows_sys::Win32::Storage::FileSystem::FILE_APPEND_DATA,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            ptr::null(),
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            ptr::null_mut(),
        )
    };
    if file == INVALID_HANDLE_VALUE {
        return;
    }
    if out_missing {
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, file) };
    }
    if err_missing {
        unsafe { SetStdHandle(STD_ERROR_HANDLE, file) };
    }
}

#[cfg(not(windows))]
fn hook_std_handles(_log: Option<&Path>) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test's warnings are its own: one another thread raises (another
    /// test's, under the harness) is neither counted nor remembered here.
    #[test]
    fn a_tests_warnings_are_its_threads_own() {
        let (before, _) = warnings_since(0);
        std::thread::spawn(|| note_warning("another test's".into()))
            .join()
            .unwrap();
        assert_eq!(warnings_since(before), (before, None));
        note_warning("this test's".into());
        assert_eq!(
            warnings_since(before),
            (before + 1, Some("this test's".into()))
        );
    }

    #[test]
    fn writing_before_init_is_harmless() {
        write("no log yet");
    }

    #[test]
    fn the_log_name_is_arklay_log() {
        assert_eq!(
            crate::paths::log_path()
                .file_name()
                .and_then(|name| name.to_str()),
            Some("arklay.log")
        );
    }
}
