#![allow(dead_code)]
//#![allow(unused_variables)]
//#![allow(unused_imports)]

mod cloudwatch;
pub mod config;
mod metrics;
mod publisher;
mod memory;

use log::{debug, error, info, warn};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::signal::unix as signal_unix;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::Mutex as TokioMutex;

use crate::cloudwatch::create_cloudwatch_publisher;
use crate::config::CloudwatchConfig;
use crate::metrics::*;
use crate::publisher::{ConsolePublisher, MetricPublisher};

/// How often collect samples
const MEASUREMENT_PERIOD: Duration = Duration::from_millis(900);

/// Message between collector task and publisher task
#[derive(Debug)]
pub enum PublisherMessage {
    /// Metric aggregated measurements
    Metric(Measurement),
    /// Request to shutdown
    Quit,
}

/// Message between collector task and heartbeat task
#[derive(Debug)]
pub enum CollectorMessage {
    Aggregation,
    Quit,
}

/// Task for collecting metrics
async fn metrics_collector(
    tx: mpsc::Sender<PublisherMessage>,
    rx_aggregation: &mut mpsc::Receiver<CollectorMessage>,
) {
    let mut sys = create_measurement_engine();

    let mut series: Vec<Measurement> = vec![];

    loop {
        debug!("Metric tick");

        let measurement = create_measurement(&mut sys);
        series.push(measurement);

        match rx_aggregation.try_recv() {
            Ok(message) => {
                match message {
                    CollectorMessage::Aggregation => {
                        if let Some(aggregated_measurement) = aggregate(&series) {
                            series.clear();
                            // now send
                            if let Err(err) = tx
                                .send(PublisherMessage::Metric(aggregated_measurement))
                                .await
                            {
                                error!("Send to metric channel error: {}", err);
                                break;
                            }
                        }
                    }
                    CollectorMessage::Quit => {
                        info!("Requested to quit");
                        break;
                    }
                }
            }
            Err(TryRecvError::Empty) => (),
            Err(TryRecvError::Disconnected) => {
                warn!("Aggregation channel disconnected");
            }
        };

        tokio::time::sleep(MEASUREMENT_PERIOD).await;
    }
    info!("Collector finished");
}

/// Task for publishing metrics
async fn metrics_publisher(
    rx: &mut mpsc::Receiver<PublisherMessage>,
    publisher: &Arc<TokioMutex<dyn MetricPublisher + Send + Sync>>,
) {
    while let Some(message) = rx.recv().await {
        match message {
            PublisherMessage::Metric(measurement) => {
                debug!("Received {:?}", measurement);
                let mut ref_publisher = publisher.lock().await;
                let res = ref_publisher.send(measurement).await;
                if let Err(err) = res {
                    error!("Failed to send metrics: {}", err);
                }
            }
            PublisherMessage::Quit => {
                info!("Exiting receiver");
                break;
            }
        }
    }
    info!("Publisher finished");
}

pub async fn handle_shutdown(
    tx_collector_shutdown: mpsc::Sender<CollectorMessage>,
    tx_publisher_shutdown: mpsc::Sender<PublisherMessage>,
    rx_additional_shutdown: &mut mpsc::Receiver<()>,
    collector_task: tokio::task::JoinHandle<()>,
    publisher_task: tokio::task::JoinHandle<()>,
) -> Result<(), aws_sdk_cloudwatch::Error> {
    // stream of SIGTERM signals
    let mut stream_sigterm = signal_unix::signal(signal_unix::SignalKind::terminate()).unwrap();
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = stream_sigterm.recv() => {},
        _ = rx_additional_shutdown.recv() => {},
    };

    info!("Got terminate condition");

    // Try to aggregate last time
    info!("Aggregate last time");
    tx_collector_shutdown
        .send(CollectorMessage::Aggregation)
        .await
        .unwrap();
    tx_collector_shutdown
        .send(CollectorMessage::Quit)
        .await
        .unwrap();
    let _ = collector_task.await;

    // Wait for publisher
    info!("Wait for publisher task completion...");
    tx_publisher_shutdown
        .send(PublisherMessage::Quit)
        .await
        .unwrap();
    let _ = publisher_task.await;

    info!("All tasks completed");

    Ok(())
}

/// Entry point that orchestrate tasks and shutdown
pub async fn main_runner(
    cloudwatch_config: CloudwatchConfig,
    dryrun: bool,
    period: u32,
) -> Result<(), aws_sdk_cloudwatch::Error> {
    let (tx_metric, mut rx_metric) = mpsc::channel(4);
    let tx_publisher_shutdown = tx_metric.clone();

    let (tx_aggregation, mut rx_aggregation) = mpsc::channel(4);
    let tx_collector_shutdown = tx_aggregation.clone();

    let collector_task = tokio::spawn(async move {
        metrics_collector(tx_metric, &mut rx_aggregation).await;
    });

    let _aggregation_heartbeat_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(period as u64)).await;
            if let Err(err) = tx_aggregation.send(CollectorMessage::Aggregation).await {
                error!("Cannot send Aggregation message to collector: {}", err);
            }
        }
    });

    // create a publisher implementation
    let publisher: Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> = if dryrun {
        Arc::new(TokioMutex::new(ConsolePublisher {}))
    } else {
        Arc::new(TokioMutex::new(
            create_cloudwatch_publisher(cloudwatch_config).await,
        ))
    };

    let publisher_task = tokio::spawn(async move {
        metrics_publisher(&mut rx_metric, &publisher).await;
    });

    info!("Started all tasks");

    let (_tx, mut rx_additional_shutdown) = mpsc::channel(1);
    handle_shutdown(
        tx_collector_shutdown,
        tx_publisher_shutdown,
        &mut rx_additional_shutdown,
        collector_task,
        publisher_task,
    )
    .await?;
    Ok(())
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use more_asserts::*;
    use test_log::test;

    /// Check collecting metrics
    #[test(tokio::test)]
    async fn test_collector_multiple() {
        let (tx_metric, mut rx_metric) = mpsc::channel(4);
        let (tx_aggregation, mut rx_aggregation) = mpsc::channel(4);

        let collect_task = tokio::spawn(async move {
            metrics_collector(tx_metric, &mut rx_aggregation).await;
        });
        // receive emitted measurements
        let received: Arc<TokioMutex<Vec<Measurement>>> = Arc::new(TokioMutex::new(vec![]));
        let received_for_task = received.clone();
        let consumer_task = tokio::spawn(async move {
            while let Some(message) = rx_metric.recv().await {
                if let PublisherMessage::Metric(measurement) = message {
                    received_for_task.lock().await.push(measurement);
                }
            }
        });
        tokio::time::sleep(Duration::from_secs(5)).await;
        // force aggregation
        let _ = tx_aggregation.send(CollectorMessage::Aggregation).await;
        let _ = tx_aggregation.send(CollectorMessage::Quit).await;
        let _ = collect_task.await;
        let _ = consumer_task.await;

        let messages = received.lock().await;
        assert_eq!(messages.len(), 1);
        assert_ge!(messages[0].sample_count, 3);
    }

    struct FakePublisher {
        measurements: Vec<Measurement>,
    }

    #[async_trait]
    impl MetricPublisher for FakePublisher {
        async fn send(
            &mut self,
            measurement: Measurement,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.measurements.push(measurement);
            Ok(())
        }
    }

    /// Check publishing to a fake publisher
    #[test(tokio::test)]
    async fn test_publish() {
        let (tx_metric, mut rx_metric) = mpsc::channel(4);
        let (tx_aggregation, mut rx_aggregation) = mpsc::channel(4);

        let tx2 = tx_metric.clone();
        let collect_task = tokio::spawn(async move {
            metrics_collector(tx_metric, &mut rx_aggregation).await;
        });
        let fake_publisher = Arc::new(TokioMutex::new(FakePublisher {
            measurements: vec![],
        }));
        let publisher: Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> = fake_publisher.clone();

        let publisher_task = tokio::spawn(async move {
            metrics_publisher(&mut rx_metric, &publisher).await;
        });

        for _ in 0..3 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let _ = tx_aggregation.send(CollectorMessage::Aggregation).await;
        }
        let _ = tx_aggregation.send(CollectorMessage::Quit).await;
        let _ = collect_task.await;
        let _ = tx2.send(PublisherMessage::Quit).await;
        let _ = publisher_task.await;

        let ref_publisher = &fake_publisher.lock().await;
        assert_eq!(ref_publisher.measurements.len(), 3);
        assert_ge!(ref_publisher.measurements[0].sample_count, 2);
    }

    struct FailurePublisher {
        counter: u32,
        measurements: Vec<Measurement>,
    }

    #[async_trait]
    impl MetricPublisher for FailurePublisher {
        async fn send(
            &mut self,
            measurement: Measurement,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.counter += 1;
            if (self.counter % 2) == 0 {
                return Err(Box::new(std::env::VarError::NotPresent));
            }
            self.measurements.push(measurement);
            Ok(())
        }
    }

    /// Ensures that failure in submitting metrics does not stop publisher
    #[test(tokio::test)]
    async fn test_publish_fails() {
        let (tx_metric, mut rx_metric) = mpsc::channel(4);
        let (tx_aggregation, mut rx_aggregation) = mpsc::channel(4);

        let tx2 = tx_metric.clone();

        let collect_task = tokio::spawn(async move {
            metrics_collector(tx_metric, &mut rx_aggregation).await;
        });
        let failure_publisher = Arc::new(TokioMutex::new(FailurePublisher {
            counter: 0,
            measurements: vec![],
        }));
        let publisher: Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> =
            failure_publisher.clone();

        let publisher_task = tokio::spawn(async move {
            metrics_publisher(&mut rx_metric, &publisher).await;
        });

        for _ in 0..3 {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let _ = tx_aggregation.send(CollectorMessage::Aggregation).await;
        }
        let _ = tx_aggregation.send(CollectorMessage::Quit).await;
        let _ = collect_task.await;
        let _ = tx2.send(PublisherMessage::Quit).await;
        let _ = publisher_task.await;

        let ref_publisher = &failure_publisher.lock().await;
        assert_ge!(ref_publisher.measurements.len(), 2);
        assert_ge!(ref_publisher.counter, 2);
    }

    /// Check publishing to a fake publisher
    #[test(tokio::test)]
    async fn test_publish_remaining() {
        let (tx_metric, mut rx_metric) = mpsc::channel(4);
        let (tx_aggregation, mut rx_aggregation) = mpsc::channel(4);

        let tx_publisher_shutdown = tx_metric.clone();
        let tx_collector_shutdown = tx_aggregation.clone();

        let collect_task = tokio::spawn(async move {
            metrics_collector(tx_metric, &mut rx_aggregation).await;
        });
        let fake_publisher = Arc::new(TokioMutex::new(FakePublisher {
            measurements: vec![],
        }));
        let publisher: Arc<TokioMutex<dyn MetricPublisher + Send + Sync>> = fake_publisher.clone();

        let publisher_task = tokio::spawn(async move {
            metrics_publisher(&mut rx_metric, &publisher).await;
        });

        tokio::time::sleep(Duration::from_secs(5)).await;
        let _ = tx_aggregation.send(CollectorMessage::Aggregation).await;
        tokio::time::sleep(Duration::from_secs(5)).await;

        // force shutdown without receiving signals
        let (tx_additional_shutdown, mut rx_additional_shutdown) = mpsc::channel(1);
        tx_additional_shutdown.send(()).await.unwrap();

        handle_shutdown(
            tx_collector_shutdown,
            tx_publisher_shutdown,
            &mut rx_additional_shutdown,
            collect_task,
            publisher_task,
        )
        .await
        .unwrap();

        // there should be two messages - one after explicit Aggregation,
        // second due to shutdown
        let ref_publisher = &fake_publisher.lock().await;
        assert_eq!(ref_publisher.measurements.len(), 2);
    }
}
