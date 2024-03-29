use log::debug;
use std::fs::File;
use std::io::BufRead;
use sysinfo::{System, SystemExt};

pub struct MemoryMeasurement {
    pub utilization: f64,
    pub max_utilization: f64,
}

fn read_cgroups_v1_usage() -> Result<u64, Box<dyn std::error::Error>> {
    let err = std::io::Error::from(std::io::ErrorKind::NotFound);
    if let Ok(file) = File::open("/sys/fs/cgroup/memory/memory.usage_in_bytes") {
        // file content is a value in bytes
        return Ok(std::io::BufReader::new(file)
            .lines()
            .next()
            .ok_or_else(|| Box::new(err))??
            .parse::<u64>()?);
    }
    Err(Box::new(err))
}

fn read_cgroups_v1_max_usage() -> Result<u64, Box<dyn std::error::Error>> {
    let err = std::io::Error::from(std::io::ErrorKind::NotFound);
    if let Ok(file) = File::open("/sys/fs/cgroup/memory/memory.max_usage_in_bytes") {
        // file content is a value in bytes
        return Ok(std::io::BufReader::new(file)
            .lines()
            .next()
            .ok_or_else(|| Box::new(err))??
            .parse::<u64>()?);
    }
    Err(Box::new(err))
}

fn read_cgroups_v1_limit() -> Result<u64, Box<dyn std::error::Error>> {
    let err = std::io::Error::from(std::io::ErrorKind::NotFound);
    if let Ok(file) = File::open("/sys/fs/cgroup/memory/memory.stat") {
        if let Some(hier_line) = std::io::BufReader::new(file)
            .lines()
            .map_while(Result::ok)
            .find(|s| s.starts_with("hierarchical_memory_limit "))
        {
            // line format is:
            // hierarchical_memory_limit 12345
            // where the last value is a soft memory limit in bytes
            let value = hier_line
                .split_whitespace()
                .last()
                .unwrap()
                .parse::<u64>()?;
            if value < 0x7FFFFFFFFFFF0000 {
                return Ok(value);
            } else {
                // If it contains a large value with zero bits in low 4 or 8 bits, no limit is imposed
                debug!("cgroups v1 with no memory limit: {}", value);
                return Err(Box::new(err));
            }
        }
    }
    Err(Box::new(err))
}

/// Detect system memory usage using cgroups v1
/// Works only if memory limit is set (it is a case for Fargate containers)
fn collect_memory_cgroups_v1() -> Option<MemoryMeasurement> {
    if let Ok(usage) = read_cgroups_v1_usage() {
        if let Ok(max_usage) = read_cgroups_v1_max_usage() {
            if let Ok(limit) = read_cgroups_v1_limit() {
                debug!(
                    "Got cgroups v1 memory usage {}, max {} and limit {}",
                    usage, max_usage, limit
                );
                let utilization = (usage as f64) / (limit as f64);
                let max_utilization: f64 = (max_usage as f64) / (limit as f64);
                return Some(MemoryMeasurement {
                    utilization,
                    max_utilization,
                });
            }
        }
    }
    None
}

/// Detect system memory usage using a standard memory info
fn collect_memory_sysinfo(sys: &mut System) -> MemoryMeasurement {
    let total = sys.total_memory() as f64;
    let utilization = (sys.used_memory() as f64) / total;
    let max_utilization: f64 = utilization;
    MemoryMeasurement {
        utilization,
        max_utilization,
    }
}

/// Write memory info to writer
pub fn collect_memory_info<W: std::fmt::Write>(f: &mut W, sys: &mut System) {
    writeln!(
        f,
        "Sysinfo: used memory {}, system memory {}",
        sys.used_memory(),
        sys.total_memory()
    )
    .unwrap();
    if let Ok(limit) = read_cgroups_v1_limit() {
        writeln!(f, "cgroups v1: limit {}", limit).unwrap();
    }
}

/// Detect system memory usage
pub fn collect_memory(sys: &mut System) -> MemoryMeasurement {
    if let Some(mem) = collect_memory_cgroups_v1() {
        return mem;
    }
    collect_memory_sysinfo(sys)
}
