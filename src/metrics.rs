use chrono::{DateTime, Utc};
use std::fmt;
use sysinfo::{ProcessRefreshKind, ProcessorExt, RefreshKind, System, SystemExt};

pub struct Measurement {
    pub timestamp: std::time::SystemTime,
    pub mem_utilization: f64,
    pub max_mem_utilization: f64,
    pub cpu_utilization: f64,
    pub sample_count: u32
}

impl fmt::Debug for Measurement {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dt: DateTime<Utc> = self.timestamp.into();
        write!(
            fmt,
            "Measurement {{ ts {}, cpu {:.3}, mem {:.3} }}",
            dt.to_rfc3339(),
            self.cpu_utilization,
            self.mem_utilization
        )?;
        Ok(())
    }
}

pub fn create_measurement_engine() -> System {
    let refresh_kind = RefreshKind::new()
        .with_cpu()
        .with_memory()
        .with_processes(ProcessRefreshKind::everything());
    System::new_with_specifics(refresh_kind)
}

pub fn create_measurement(sys: &mut System) -> Measurement {
    sys.refresh_cpu();
    sys.refresh_memory();
    //for p in sys.processors() {
    //println!(" cpu {}", p.cpu_usage());
    //}
    //println!();

    let cpu_count = sys.processors().len();
    let cpu_sum: f64 = sys.processors().iter().map(|p| p.cpu_usage() as f64).sum();
    let cpu_avg = if cpu_count > 0 && !cpu_sum.is_nan() {
        cpu_sum / (cpu_count as f64) / 100.0
    } else {
        0.0
    };
    let cpu_utilization: f64 = cpu_avg;

    let mem_val = (sys.used_memory() as f64) / (sys.total_memory() as f64);
    let mem_utilization: f64 = if !mem_val.is_nan() { mem_val } else { 0.0 };

    Measurement {
        timestamp: std::time::SystemTime::now(),
        cpu_utilization,
        mem_utilization,
        max_mem_utilization: mem_utilization,
        sample_count: 1
    }
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use more_asserts::*;

    #[test]
    fn test_measurement() {
        let mut engine = create_measurement_engine();
        let measurement = create_measurement(&mut engine);
        assert!(!measurement.cpu_utilization.is_nan());
        assert!(!measurement.mem_utilization.is_nan());

        assert_le!(measurement.cpu_utilization, 1.0);
        assert_le!(measurement.mem_utilization, 1.0);
    }

    #[test]
    fn test_measurement_times() {
        let mut engine = create_measurement_engine();
        for _ in 0..10 {
            let measurement = create_measurement(&mut engine);
            println!("{:?}", measurement);
            assert!(!measurement.cpu_utilization.is_nan());
            assert!(!measurement.mem_utilization.is_nan());

            assert_ge!(measurement.cpu_utilization, 0.0);
            assert_le!(measurement.cpu_utilization, 1.0);
            assert_ge!(measurement.mem_utilization, 0.0);
            assert_le!(measurement.mem_utilization, 1.0);
        }
    }
}

pub fn aggregate(series: &[Measurement]) -> Measurement {
    if series.is_empty() {
        panic!("No messages");
    }
    println!("Got aggregated {} from {} measurements", 1, series.len());
    Measurement {
        timestamp: series[0].timestamp,
        cpu_utilization: series[0].cpu_utilization,
        mem_utilization: series[0].mem_utilization,
        max_mem_utilization: series[0].max_mem_utilization,
        sample_count: series.len() as u32
    }
}


