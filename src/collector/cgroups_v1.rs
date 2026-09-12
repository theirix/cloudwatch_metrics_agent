use crate::collector::types::{MeasurementError, MemoryCollector, MemoryMeasurement};
use log::debug;
use std::fmt::Write;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn read_cgroups_v1_usage() -> Result<u64, MeasurementError> {
    let file = File::open("/sys/fs/cgroup/memory/memory.usage_in_bytes")?;

    // file content is a value in bytes
    BufReader::new(file)
        .lines()
        .next()
        .ok_or(MeasurementError::Format("No line found in sys file".into()))??
        .parse::<u64>()
        .map_err(MeasurementError::Parse)
}

fn read_cgroups_v1_max_usage() -> Result<u64, MeasurementError> {
    let file = File::open("/sys/fs/cgroup/memory/memory.max_usage_in_bytes")?;

    // file content is a value in bytes
    BufReader::new(file)
        .lines()
        .next()
        .ok_or(MeasurementError::Format("No line found in sys file".into()))??
        .parse::<u64>()
        .map_err(MeasurementError::Parse)
}

fn read_cgroups_v1_limit() -> Result<u64, MeasurementError> {
    let file = File::open("/sys/fs/cgroup/memory/memory.stat")?;

    let hier_line = BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .find(|s| s.starts_with("hierarchical_memory_limit "))
        .ok_or(MeasurementError::Format("No line found in sys file".into()))?;

    // line format is:
    // hierarchical_memory_limit 12345
    // where the last value is a soft memory limit in bytes
    let value: u64 = hier_line
        .split_whitespace()
        .last()
        .ok_or_else(|| MeasurementError::Format(format!("Cannot parse line {hier_line}")))?
        .parse()
        .map_err(MeasurementError::Parse)?;
    if value < 0x7FFFFFFFFFFF0000 {
        Ok(value)
    } else {
        // If it contains a large value with zero bits in low 4 or 8 bits, no limit is imposed
        Err(MeasurementError::Format(format!(
            "cgroups v1 with no memory limit: {value}"
        )))
    }
}

/// Detect system memory usage using cgroups v1
/// Works only if memory limit is set (it is a case for Fargate containers)
fn collect_memory_cgroups_v1() -> Result<MemoryMeasurement, MeasurementError> {
    let usage = read_cgroups_v1_usage()?;
    let max_usage = read_cgroups_v1_max_usage()?;
    let limit = read_cgroups_v1_limit()?;

    debug!(
        "Got cgroups v1 memory usage {}, max {} and limit {}",
        usage, max_usage, limit
    );
    let utilization = (usage as f64) / (limit as f64);
    let max_utilization: f64 = (max_usage as f64) / (limit as f64);
    Ok(MemoryMeasurement {
        utilization,
        max_utilization,
    })
}

pub struct CgroupsV1Collector;

impl CgroupsV1Collector {
    pub fn new() -> Self {
        Self {}
    }
}

impl MemoryCollector for CgroupsV1Collector {
    fn collect_memory(&self) -> Result<MemoryMeasurement, MeasurementError> {
        collect_memory_cgroups_v1()
    }

    fn write_info<W: Write>(&self, f: &mut W) -> Result<(), MeasurementError> {
        if let Ok(limit) = read_cgroups_v1_limit() {
            writeln!(f, "cgroups v1: limit {}", limit)?;
            Ok(())
        } else {
            Err(MeasurementError::Format("Cannot read memory info".into()))
        }
    }
}
