use crate::memory::*;

use chrono::{DateTime, Utc};
use log::*;
use rstats::mutstats::minmax;
use rstats::Stats;
use std::fmt;
use std::time::SystemTime;
use sysinfo::{ProcessRefreshKind, ProcessorExt, RefreshKind, System, SystemExt};

pub struct Measurement {
    pub timestamp: SystemTime,
    pub mem_utilization: f64,
    pub max_mem_utilization: f64,
    pub cpu_utilization: f64,
    pub sample_count: u32,
}

impl fmt::Debug for Measurement {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dt: DateTime<Utc> = self.timestamp.into();
        write!(
            fmt,
            "Measurement {{ ts {}, cpu {:.3}, mem {:.3}, maxmem {:.3} }}",
            dt.to_rfc3339(),
            self.cpu_utilization,
            self.mem_utilization,
            self.max_mem_utilization
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

    let memory_measurement = collect_memory(sys);
    let mem_val = memory_measurement.utilization;
    let mem_utilization: f64 = if !mem_val.is_nan() { mem_val } else { 0.0 };

    Measurement {
        timestamp: SystemTime::now(),
        cpu_utilization,
        mem_utilization,
        max_mem_utilization: mem_utilization,
        sample_count: 1,
    }
}

pub fn aggregate(series: &[Measurement]) -> Option<Measurement> {
    if series.is_empty() {
        return None;
    }
    debug!("Got aggregated {} from {} measurements", 1, series.len());
    let avg_cpu: f64 = series
        .iter()
        .map(|m| m.cpu_utilization)
        .collect::<Vec<f64>>()
        .median()
        .unwrap()
        .median;
    let avg_mem: f64 = series
        .iter()
        .map(|m| m.mem_utilization)
        .collect::<Vec<f64>>()
        .median()
        .unwrap()
        .median;
    let max_mem: f64 = minmax(
        &series
            .iter()
            .map(|m| m.max_mem_utilization)
            .collect::<Vec<f64>>(),
    )
    .max;
    Some(Measurement {
        timestamp: series[series.len() - 1].timestamp,
        cpu_utilization: avg_cpu,
        mem_utilization: avg_mem,
        max_mem_utilization: max_mem,
        sample_count: series.len() as u32,
    })
}

/// Write generic system info into writer
pub fn collect_info<W: std::fmt::Write>(f: &mut W, sys: &mut System) {
    sys.refresh_cpu();
    sys.refresh_memory();
    collect_memory_info(f, sys);
    writeln!(f, "Sysinfo: cpu count: {}", sys.processors().len()).unwrap();
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use more_asserts::*;
    use std::time::Duration;
    use test_log::test;

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

    #[test]
    fn test_aggregate_empty() {
        assert!(aggregate(&vec![]).is_none());
    }

    #[test]
    fn test_aggregate_multiple() {
        let base = SystemTime::now();
        let n = 10;
        let measurements: Vec<Measurement> = (0..n)
            .map(|k| {
                let ts = base + Duration::from_secs(k * 2);
                Measurement {
                    timestamp: ts,
                    cpu_utilization: k as f64 * 0.05,
                    mem_utilization: k as f64 * 0.07,
                    max_mem_utilization: k as f64 * 0.07,
                    sample_count: 1,
                }
            })
            .collect();
        let agg = aggregate(&measurements).unwrap();
        println!("Agg: {:?}", agg);
        assert_eq!(agg.sample_count, n as u32);
        // avg of series 0..(k-1)*b is (0 + k-1)*b/2 / n = (k-1)*b/2
        assert!((agg.cpu_utilization - (0.05 / 2.0 * ((n - 1) as f64))).abs() < 0.001);
        assert!((agg.mem_utilization - (0.07 / 2.0 * ((n - 1) as f64))).abs() < 0.001);
        assert!((agg.max_mem_utilization - (0.07 * (n - 1) as f64)).abs() < 0.001);
    }
}
