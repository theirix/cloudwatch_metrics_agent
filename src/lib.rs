#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

pub mod config;
mod metrics;
mod publisher;
mod cloudwatch;

use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::signal::unix as signal_unix;
use tokio::sync::mpsc;
use tokio::sync::Mutex as TokioMutex;

use crate::config::CloudwatchConfig;
use crate::metrics::{create_measurement, Measurement};
use crate::publisher::{MetricPublisher, ConsolePublisher};
use crate::cloudwatch::create_cloudwatch_publisher;

/// Enum to pass between async tasks
#[derive(Debug)]
pub enum Message {
    /// Metric measurement for one timestamp
    Measurement(Measurement),
    /// Request to shutdown
    Quit,
}

/// Task for collecting metrics
async fn metrics_collector(tx: mpsc::Sender<Message>, period: u32) {
    loop {
        println!("Metric tick");

        let measurement = create_measurement();

        if let Err(err) = tx.send(Message::Measurement(measurement)).await {
            eprintln!("Send error: {}", err);
            break;
        };

        tokio::time::sleep(Duration::from_secs(period as u64)).await;
    }
    println!("Collector finished");
}

/// Task for publishing metrics
async fn metrics_publisher(
    rx: &mut mpsc::Receiver<Message>,
    publisher: &Arc<TokioMutex<dyn MetricPublisher + Send + Sync>>,
) {
    while let Some(message) = rx.recv().await {
        match message {
            Message::Measurement(measurement) => {
                println!("Received {:?}", measurement);
                let mut ref_publisher = publisher.lock().await;
                let res = ref_publisher.send(measurement).await;
                if let Err(err) = res {
                    eprintln!("Failed to send metrics: {}", err);
                }
            }
            Message::Quit => {
                println!("Exiting receiver");
                break;
            }
        }
    }
    println!("Publisher finished");
}

pub async fn handle_shutdown(
    tx_collector_shutdown: mpsc::Sender<Message>,
    sender_task: tokio::task::JoinHandle<()>,
) -> Result<(), aws_sdk_cloudwatch::Error> {
    // stream of SIGTERM signals
    let mut stream_sigterm = signal_unix::signal(signal_unix::SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = stream_sigterm.recv() => {},
    };

    println!("Got terminate condition");
    tx_collector_shutdown.send(Message::Quit).await.unwrap();
    // wait completion
    println!("Wait for sender task completion...");
    let _ = sender_task.await;

    println!("All tasks completed");

    Ok(())
}

/// Entry point that orchestrate tasks and shutdown
pub async fn main_runner(
    cloudwatch_config: CloudwatchConfig,
    dryrun: bool,
    period: u32,
) -> Result<(), aws_sdk_cloudwatch::Error> {
    let (tx_collector, mut rx_collector) = mpsc::channel(4);
    let tx_collector_shutdown = tx_collector.clone();

    let _collect_task = tokio::spawn(async move {
        metrics_collector(tx_collector, period).await;
    });

    // create a publisher implementation
    let publisher: Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> = if dryrun {
        Arc::new(TokioMutex::new(ConsolePublisher {}))
    } else {
        Arc::new(TokioMutex::new(create_cloudwatch_publisher(cloudwatch_config).await))
    };

    let sender_task = tokio::spawn(async move {
        metrics_publisher(&mut rx_collector, &publisher).await;
    });

    println!("Started all tasks");

    handle_shutdown(tx_collector_shutdown, sender_task).await?;
    Ok(())
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use more_asserts::*;
    use async_trait::async_trait;

    /// Check collecting metrics
    #[tokio::test]
    async fn test_collector_multiple() {
        let (tx_collector, mut rx_collector) = mpsc::channel(4);
        let collect_task = tokio::spawn(async move {
            metrics_collector(tx_collector, 1).await;
        });
        // receive emitted measurements
        let received: Arc<TokioMutex<Vec<Measurement>>> = Arc::new(TokioMutex::new(vec![]));
        let received_for_task = received.clone();
        tokio::spawn(async move {
            while let Some(message) = rx_collector.recv().await {
                if let Message::Measurement(measurement) = message {
                    received_for_task.lock().await.push(measurement);
                }
            }
        });
        tokio::time::sleep(Duration::from_secs(5)).await;
        drop(collect_task);
        assert_gt!(received.lock().await.len(), 3);
    }

    struct FakePublisher {
        measurements: Vec<Measurement>
    }

    #[async_trait]
    impl MetricPublisher for FakePublisher {
        async fn send(&mut self, measurement: Measurement) -> Result<(), Box<dyn std::error::Error>> {
            self.measurements.push(measurement);
            Ok(())
        }
    }

    /// Check publishing to a fake publisher
    #[tokio::test]
    async fn test_publish() {
        let (tx_collector, mut rx_collector) = mpsc::channel(4);
        let tx2 = tx_collector.clone();
        let _collect_task = tokio::spawn(async move {
            metrics_collector(tx_collector, 1).await;
        });
        let fake_publisher = Arc::new(TokioMutex::new(FakePublisher{ measurements: vec![] }));
        let publisher : Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> = fake_publisher.clone();

        let sender_task = tokio::spawn(async move {
            metrics_publisher(&mut rx_collector, &publisher).await;
        });

        tokio::time::sleep(Duration::from_secs(5)).await;
        let _ = tx2.send(Message::Quit).await;
        let _ = sender_task.await;
        
        let ref_publisher = &fake_publisher.lock().await;
        assert_gt!(ref_publisher.measurements.len(), 3);
    }
    
    struct FailurePublisher {
        counter: u32,
        measurements: Vec<Measurement>
    }

    #[async_trait]
    impl MetricPublisher for FailurePublisher {
        async fn send(&mut self, measurement: Measurement) -> Result<(), Box<dyn std::error::Error>> {
            self.counter += 1;
            if (self.counter % 2) == 0 {
                return Err(Box::new(std::env::VarError::NotPresent));
            }
            self.measurements.push(measurement);
            Ok(())
        }
    }

    /// Ensures that failure in submitting metrics does not stop publisher
    #[tokio::test]
    async fn test_publish_fails() {
        let (tx_collector, mut rx_collector) = mpsc::channel(4);
        let tx2 = tx_collector.clone();
        let _collect_task = tokio::spawn(async move {
            metrics_collector(tx_collector, 1).await;
        });
        let failure_publisher = Arc::new(TokioMutex::new(FailurePublisher{ counter: 0, measurements: vec![] }));
        let publisher : Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> = failure_publisher.clone();

        let sender_task = tokio::spawn(async move {
            metrics_publisher(&mut rx_collector, &publisher).await;
        });

        tokio::time::sleep(Duration::from_secs(5)).await;
        let _ = tx2.send(Message::Quit).await;
        let _ = sender_task.await;

        let ref_publisher = &failure_publisher.lock().await;
        assert_ge!(ref_publisher.measurements.len(), 1);
        assert_ge!(ref_publisher.counter, 1);
    }

    
}
