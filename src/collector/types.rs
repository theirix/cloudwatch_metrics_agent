use std::num::ParseIntError;

pub struct CpuMeasurement {
    pub utilization: f64,
}

pub struct MemoryMeasurement {
    pub utilization: f64,
    pub max_utilization: f64,
}

pub struct SystemMeasurement {
    pub cpu: CpuMeasurement,
    pub memory: MemoryMeasurement,
}

#[derive(thiserror::Error, Debug)]
pub enum MeasurementError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Output error: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] ParseIntError),
    #[error("Invalid format: {0}")]
    Format(String),
}

/// Abstract trait for collecting memory metrics
pub trait MemoryCollector {
    fn collect_memory(&self) -> Result<MemoryMeasurement, MeasurementError>;
    fn write_info<W: std::fmt::Write>(&self, f: &mut W) -> Result<(), MeasurementError>;
}
