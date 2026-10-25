//! The running process's memory and CPU use, for the F6 debug HUD.
//!
//! Windows reads the working set and process times from the OS; Linux reads
//! `/proc/self`. Elsewhere every reading is `None` and the overlay leaves the
//! line out.

use std::time::Instant;

/// One memory reading, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Memory {
    /// The resident (working) set.
    pub resident: u64,
    /// The largest resident set so far.
    pub peak: u64,
    /// The machine's physical memory, when known.
    pub total: Option<u64>,
}

/// Samples the process's CPU time between readings.
#[derive(Debug, Default)]
pub struct CpuMeter {
    /// The CPU time and wall clock of the previous reading.
    last: Option<(f64, Instant)>,
    /// The last computed share.
    share: Option<f32>,
}

impl CpuMeter {
    /// The share of the whole machine's CPU (every core, 0..=100) the
    /// process used since the previous call at least `min_seconds` ago; the
    /// last share in between, so a fast caller does not read noise.
    pub fn sample(&mut self, min_seconds: f64) -> Option<f32> {
        let now = Instant::now();
        let cpu = cpu_seconds()?;
        match self.last {
            Some((last_cpu, last_at)) => {
                let wall = now.duration_since(last_at).as_secs_f64();
                if wall >= min_seconds {
                    let share = (cpu - last_cpu) / wall / threads() as f64 * 100.0;
                    self.share = Some(share.clamp(0.0, 100.0) as f32);
                    self.last = Some((cpu, now));
                }
            }
            None => self.last = Some((cpu, now)),
        }
        self.share
    }
}

/// The machine's logical processors.
pub fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

#[cfg(windows)]
pub fn memory() -> Option<Memory> {
    use windows_sys::Win32::System::ProcessStatus::{
        K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    // SAFETY: the pseudo handle needs no closing and `counters` is a
    // correctly sized out-parameter.
    if unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb) } == 0 {
        return None;
    }
    let mut status = MEMORYSTATUSEX {
        dwLength: size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // SAFETY: `status` is a correctly sized out-parameter.
    let total = (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullTotalPhys);
    Some(Memory {
        resident: counters.WorkingSetSize as u64,
        peak: counters.PeakWorkingSetSize as u64,
        total,
    })
}

#[cfg(windows)]
fn cpu_seconds() -> Option<f64> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

    let mut times = [FILETIME::default(); 4];
    let [created, exited, kernel, user] = &mut times;
    // SAFETY: four valid out-parameters for the current process.
    if unsafe { GetProcessTimes(GetCurrentProcess(), created, exited, kernel, user) } == 0 {
        return None;
    }
    // FILETIME counts 100 ns intervals.
    let ticks =
        |time: &FILETIME| u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime);
    Some((ticks(kernel) + ticks(user)) as f64 / 1e7)
}

#[cfg(target_os = "linux")]
pub fn memory() -> Option<Memory> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |text: &str, name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| {
                rest.trim()
                    .trim_end_matches("kB")
                    .trim()
                    .parse::<u64>()
                    .ok()
            })
            .map(|kb| kb * 1024)
    };
    let total = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|info| field(&info, "MemTotal:"));
    Some(Memory {
        resident: field(&status, "VmRSS:")?,
        peak: field(&status, "VmHWM:")?,
        total,
    })
}

#[cfg(target_os = "linux")]
fn cpu_seconds() -> Option<f64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // The command name may hold spaces; the fields after it do not.
    let rest = &stat[stat.rfind(')')? + 2..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // utime and stime, fields 14 and 15, in clock ticks (100 Hz on Linux).
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((utime + stime) as f64 / 100.0)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn memory() -> Option<Memory> {
    None
}

#[cfg(not(any(windows, target_os = "linux")))]
fn cpu_seconds() -> Option<f64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn the_process_has_resident_memory() {
        let memory = memory().expect("a memory reading");
        assert!(memory.resident > 0);
        assert!(memory.peak >= memory.resident);
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn the_cpu_meter_needs_two_readings() {
        let mut meter = CpuMeter::default();
        assert_eq!(meter.sample(0.0), None);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let share = meter.sample(0.0).expect("a share");
        assert!((0.0..=100.0).contains(&share));
    }
}
