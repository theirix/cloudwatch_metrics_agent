use crate::collector::types::{MeasurementError, MemoryMeasurement};
use sysinfo::System;

pub fn collect_memory_info(sys: &System) -> Result<MemoryMeasurement, MeasurementError> {
    let total = sys.total_memory() as f64;
    let utilization = (sys.used_memory() as f64) / total;
    let max_utilization: f64 = utilization;
    Ok(MemoryMeasurement {
        utilization,
        max_utilization,
    })
}
