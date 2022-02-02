use crate::metrics::Measurement;

use async_trait::async_trait;

/// Generic trait
#[async_trait]
pub trait MetricPublisher {
    async fn send(&mut self, measurement: Measurement
                  ) -> Result<(), Box<dyn std::error::Error>>;
}

/// Sink implementation that just logs metrics
pub struct ConsolePublisher {}

#[async_trait]
impl MetricPublisher for ConsolePublisher {
    async fn send(&mut self, measurement: Measurement) -> Result<(), Box<dyn std::error::Error>> {
        println!("Sending measurement to console {:?}", measurement);
        Ok(())
    }
}

