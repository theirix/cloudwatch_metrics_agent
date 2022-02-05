use crate::config::CloudwatchConfig;
use crate::metrics::Measurement;
use crate::publisher::MetricPublisher;

use async_trait::async_trait;
use aws_config::meta::region::RegionProviderChain;
use log::info;

/// Sink implementation that sends metrics to Cloudwatch
pub struct CloudwatchPublisher {
    client: aws_sdk_cloudwatch::Client,
    config: CloudwatchConfig,
}

pub async fn create_cloudwatch_publisher(config: CloudwatchConfig) -> CloudwatchPublisher {
    CloudwatchPublisher {
        client: create_client(&config).await,
        config,
    }
}

async fn create_client(_config: &CloudwatchConfig) -> aws_sdk_cloudwatch::Client {
    let region_provider = RegionProviderChain::default_provider();
    let shared_config = aws_config::from_env().region(region_provider).load().await;
    aws_sdk_cloudwatch::Client::new(&shared_config)
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
            aws_sdk_cloudwatch::model::MetricDatum::builder()
                .dimensions(
                    aws_sdk_cloudwatch::model::Dimension::builder()
                        .name("ServiceName")
                        .value(&self.config.service_name)
                        .build(),
                )
                .metric_name("CPUUtilization")
                .value(measurement.cpu_utilization)
                .timestamp(measurement.timestamp.into())
                .unit(aws_sdk_cloudwatch::model::StandardUnit::Percent)
                .build(),
        );
        request_builder = request_builder.metric_data(
            aws_sdk_cloudwatch::model::MetricDatum::builder()
                .dimensions(
                    aws_sdk_cloudwatch::model::Dimension::builder()
                        .name("ServiceName")
                        .value(&self.config.service_name)
                        .build(),
                )
                .metric_name("MemoryUtilization")
                .value(measurement.mem_utilization)
                .timestamp(measurement.timestamp.into())
                .unit(aws_sdk_cloudwatch::model::StandardUnit::Percent)
                .build(),
        );
        if let Err(err) = request_builder.send().await {
            Err(err.into())
        } else {
            Ok(())
        }
    }
}

