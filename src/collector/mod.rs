mod cgroups_v1;
mod sys_info;
mod types;

use crate::collector::cgroups_v1::CgroupsV1Collector;
use crate::collector::sys_info::collect_memory_info;
pub(crate) use crate::collector::types::{
    CpuMeasurement, MemoryCollector, MemoryMeasurement, SystemMeasurement,
};
use log::debug;
use std::fmt::Write;
use sysinfo::System;
pub use types::MeasurementError;

/// Collector for collecting metrics
pub struct Collector {
    sys: System,
    cgroups_v1: CgroupsV1Collector,
}

impl MemoryCollector for Collector {
    fn collect_memory(&self) -> Result<MemoryMeasurement, MeasurementError> {
        match self.cgroups_v1.collect_memory() {
            Ok(mem) => Ok(mem),
            Err(e) => {
                debug!("Failed to collect memory using cgroups: {e:?}, fall back to sysinfo");
                collect_memory_info(&self.sys)
            }
        }
    }

    fn write_info<W: Write>(&self, buf: &mut W) -> Result<(), MeasurementError> {
        writeln!(
            buf,
            "Sysinfo: used memory {}, system memory {}",
            self.sys.used_memory(),
            self.sys.total_memory()
        )
        .expect("cannot write");

        if let Err(err) = self.cgroups_v1.write_info(buf) {
            writeln!(buf, "<cannot gather cgroups v1 info> {err}").expect("cannot write");
        }
        Ok(())
    }
}

impl Collector {
    /// Create a new collector using the given [`System`] instance
    pub fn new(sys: System) -> Self {
        Self {
            sys,
            cgroups_v1: CgroupsV1Collector::new(),
        }
    }

    fn collect_cpu(&self) -> Result<CpuMeasurement, MeasurementError> {
        let cpu_count = self.sys.cpus().len();
        let cpu_sum: f64 = self.sys.cpus().iter().map(|p| p.cpu_usage() as f64).sum();
        let cpu_avg = if cpu_count > 0 && !cpu_sum.is_nan() {
            cpu_sum / (cpu_count as f64) / 100.0
        } else {
            0.0
        };
        let utilization: f64 = cpu_avg;
        Ok(CpuMeasurement { utilization })
    }

    pub fn collect_system(&self) -> Result<SystemMeasurement, MeasurementError> {
        let cpu_measurement = self.collect_cpu()?;
        let mem_measurement = self.collect_memory()?;
        Ok(SystemMeasurement {
            cpu: cpu_measurement,
            memory: mem_measurement,
        })
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
    }
}
