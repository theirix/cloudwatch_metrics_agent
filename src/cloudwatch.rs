use crate::config::CloudwatchConfig;
use crate::metrics::Measurement;
use crate::publisher::MetricPublisher;

use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;
use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};
use aws_sdk_cloudwatch::Client;
use log::info;

/// Sink implementation that sends metrics to Cloudwatch
pub struct CloudwatchPublisher {
    client: Client,
    config: CloudwatchConfig,
}

pub async fn create_cloudwatch_publisher(config: CloudwatchConfig) -> CloudwatchPublisher {
    CloudwatchPublisher {
        client: create_client(&config).await,
        config,
    }
}

async fn create_client(_config: &CloudwatchConfig) -> Client {
    let region_provider = RegionProviderChain::default_provider();
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await;
    Client::new(&shared_config)
}

#[async_trait]
impl MetricPublisher for CloudwatchPublisher {
    async fn send(&mut self, measurement: Measurement) -> Result<(), Box<dyn std::error::Error>> {
        info!("Sending measurement to CloudWatch {:?}", measurement);

        let mut request_builder = self
            .client
            .put_metric_data()
            .namespace(&self.config.namespace);

        request_builder = request_builder.metric_data(
            MetricDatum::builder()
                .dimensions(
                    Dimension::builder()
                        .name("ServiceName")
                        .value(&self.config.service_name)
                        .build(),
                )
                .metric_name("CPUUtilization")
                .value(measurement.cpu_utilization)
                .timestamp(measurement.timestamp.into())
                .unit(StandardUnit::Percent)
                .build(),
        );
        request_builder = request_builder.metric_data(
            MetricDatum::builder()
                .dimensions(
                    Dimension::builder()
                        .name("ServiceName")
                        .value(&self.config.service_name)
                        .build(),
                )
                .metric_name("MemoryUtilization")
                .value(measurement.mem_utilization)
                .timestamp(measurement.timestamp.into())
                .unit(StandardUnit::Percent)
                .build(),
        );
        request_builder = request_builder.metric_data(
            MetricDatum::builder()
                .dimensions(
                    Dimension::builder()
                        .name("ServiceName")
                        .value(&self.config.service_name)
                        .build(),
                )
                .metric_name("MaxMemoryUtilization")
                .value(measurement.max_mem_utilization)
                .timestamp(measurement.timestamp.into())
                .unit(StandardUnit::Percent)
                .build(),
        );
        if let Err(err) = request_builder.send().await {
            Err(err.into())
        } else {
            Ok(())
        }
    }
}
