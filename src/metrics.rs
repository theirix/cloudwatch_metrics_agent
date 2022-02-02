use sysinfo::{System, SystemExt};
use std::fmt;
use chrono::{DateTime, Utc};

pub struct Measurement {
    pub timestamp: std::time::SystemTime,
    pub mem_utilization: f64,
    pub cpu_utilization: f64,
}

impl fmt::Debug for Measurement {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        let dt: DateTime<Utc> = self.timestamp.into();
        write!(fmt, "Measurement {{ ts {}, cpu {:.3}, mem {:.3} }}", dt.to_rfc3339(),
        self.cpu_utilization, self.mem_utilization)?;
        Ok(())
    }
}

pub fn create_measurement() -> Measurement {
    let mut sys = System::new_all();
    sys.refresh_all();

    //RAM and swap information:
    //println!("total memory: {} KB", sys.total_memory());
    //println!("used memory : {} KB", sys.used_memory());
    //println!("total swap  : {} KB", sys.total_swap());
    //println!("used swap   : {} KB", sys.used_swap());

    let cpu_utilization: f64 = 0.5;
    let mem_utilization: f64 = (sys.used_memory() as f64) / (sys.total_memory() as f64);

    Measurement {
        timestamp: std::time::SystemTime::now(),
        cpu_utilization,
        mem_utilization,
    }
}


/// Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_measurement() {
        let measurement = create_measurement();
        assert!(measurement.cpu_utilization <= 1.0);
        assert!(measurement.mem_utilization <= 1.0);
    }
}
