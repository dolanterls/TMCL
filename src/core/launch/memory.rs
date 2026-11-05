use crate::core::config::models::MemoryMode;
use crate::core::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemMemory {
    pub total_mb: u64,
    pub available_mb: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPlan {
    pub minimum_mb: u32,
    pub maximum_mb: u32,
}

pub fn resolve(
    mode: MemoryMode,
    minimum_mb: u32,
    maximum_mb: u32,
    system: SystemMemory,
) -> Result<MemoryPlan> {
    match mode {
        MemoryMode::Automatic => Ok(recommend(system)),
        MemoryMode::Manual => validate_manual(minimum_mb, maximum_mb, system),
    }
}

pub fn recommend(system: SystemMemory) -> MemoryPlan {
    let usable = system.available_mb.min(system.total_mb).max(1024);
    let maximum = (usable / 2).clamp(2048, 8192);
    let maximum = maximum.min(u64::from(u32::MAX)) as u32;
    let minimum = (u64::from(maximum) / 4).clamp(1024, u64::from(maximum)) as u32;
    MemoryPlan {
        minimum_mb: minimum,
        maximum_mb: maximum.max(minimum),
    }
}

pub fn detect() -> SystemMemory {
    detect_platform()
}

fn validate_manual(minimum_mb: u32, maximum_mb: u32, system: SystemMemory) -> Result<MemoryPlan> {
    if minimum_mb == 0 || maximum_mb == 0 {
        return Err(Error::Config(
            "memory values must be greater than zero".into(),
        ));
    }
    if minimum_mb > maximum_mb {
        return Err(Error::Config(
            "minimum memory cannot exceed maximum memory".into(),
        ));
    }
    if u64::from(maximum_mb) > system.total_mb {
        return Err(Error::Config(format!(
            "maximum memory cannot exceed system memory ({} MiB)",
            system.total_mb
        )));
    }
    Ok(MemoryPlan {
        minimum_mb,
        maximum_mb,
    })
}

#[cfg(target_os = "linux")]
fn detect_platform() -> SystemMemory {
    let total_kib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| parse_meminfo(contents.as_str(), "MemTotal:"))
        .unwrap_or(8 * 1024 * 1024);
    let available_kib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| parse_meminfo(contents.as_str(), "MemAvailable:"))
        .unwrap_or(total_kib / 2);
    SystemMemory {
        total_mb: (total_kib / 1024).max(1),
        available_mb: (available_kib / 1024).max(1),
    }
}

#[cfg(target_os = "linux")]
fn parse_meminfo(contents: &str, key: &str) -> Option<u64> {
    contents.lines().find_map(|line| {
        let value = line.strip_prefix(key)?.split_whitespace().next()?;
        value.parse().ok()
    })
}

#[cfg(windows)]
fn detect_platform() -> SystemMemory {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    // The Windows API fills the structure synchronously and does not retain the pointer.
    if unsafe { GlobalMemoryStatusEx(&mut status).is_ok() } {
        return SystemMemory {
            total_mb: (status.ullTotalPhys / 1024 / 1024).max(1),
            available_mb: (status.ullAvailPhys / 1024 / 1024).max(1),
        };
    }
    SystemMemory {
        total_mb: 8 * 1024,
        available_mb: 4 * 1024,
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn detect_platform() -> SystemMemory {
    SystemMemory {
        total_mb: 8 * 1024,
        available_mb: 4 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::{MemoryPlan, SystemMemory, recommend, resolve};
    use crate::core::config::models::MemoryMode;

    #[test]
    fn automatic_memory_is_deterministic_and_bounded() {
        let system = SystemMemory {
            total_mb: 16 * 1024,
            available_mb: 12 * 1024,
        };
        assert_eq!(
            recommend(system),
            MemoryPlan {
                minimum_mb: 1536,
                maximum_mb: 6144,
            }
        );
    }

    #[test]
    fn manual_memory_rejects_invalid_ranges() {
        let system = SystemMemory {
            total_mb: 8192,
            available_mb: 4096,
        };
        assert!(resolve(MemoryMode::Manual, 4096, 2048, system).is_err());
        assert!(resolve(MemoryMode::Manual, 1024, 16384, system).is_err());
    }

    #[test]
    fn automatic_memory_never_exceeds_u32() {
        let plan = resolve(
            MemoryMode::Automatic,
            1,
            1,
            SystemMemory {
                total_mb: u64::from(u32::MAX) * 2,
                available_mb: u64::from(u32::MAX) * 2,
            },
        )
        .expect("automatic plan");
        assert!(plan.maximum_mb >= plan.minimum_mb);
    }
}
