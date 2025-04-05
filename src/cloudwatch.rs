use crate::config::CloudwatchConfig;
use crate::metrics::Measurement;
use crate::publisher::MetricPublisher;

use async_trait::async_trait;
use aws_config::imds::client::Client as ImdsClient;
use aws_config::meta::region::RegionProviderChain;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};
use aws_sdk_cloudwatch::Client;
use log::{info, warn};
use std::collections::HashMap;

/// Sink implementation that sends metrics to Cloudwatch
pub struct CloudwatchPublisher {
    client: Client,
    config: CloudwatchConfig,
    tags: HashMap<String, String>,
}

pub async fn create_cloudwatch_publisher(config: CloudwatchConfig) -> CloudwatchPublisher {
    let aws_config = get_aws_config().await;
    info!("Using custom tags: {:?}", &config.tags);
    let mut tags: HashMap<String, String> = match get_instance_id().await {
        Some(instance_id) => HashMap::from([("InstanceId".to_string(), instance_id)]),
        None => HashMap::new(),
    };
    tags.extend(config.tags.clone());
    info!("Using tags: {:?}", &tags);
    CloudwatchPublisher {
        client: create_client(&config, &aws_config).await,
        config,
        tags,
    }
}

async fn get_aws_config() -> SdkConfig {
    let region_provider = RegionProviderChain::default_provider();
    aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await
}

async fn get_ec2_instance_id() -> Result<String, Box<dyn std::error::Error>> {
    let client = ImdsClient::builder().build();
    let response = client.get("/latest/meta-data/instance-id").await?;
    let instance_id: String = response.into();
    info!("Get instance-id: {}", &instance_id);
    Ok(instance_id)
}

async fn get_instance_id() -> Option<String> {
    match get_ec2_instance_id().await {
        Ok(instance_id) => Some(instance_id),
        Err(err) => {
            warn!("Cannot get EC2 instance id: {}", &err);
            None
        }
    }
}

async fn create_client(_config: &CloudwatchConfig, aws_config: &SdkConfig) -> Client {
    Client::new(aws_config)
}

#[async_trait]
impl MetricPublisher for CloudwatchPublisher {
    async fn send(&mut self, measurement: Measurement) -> Result<(), Box<dyn std::error::Error>> {
        info!("Sending measurement to CloudWatch {:?}", measurement);

        let mut request_builder = self
            .client
            .put_metric_data()
            .namespace(&self.config.namespace);

        let mut metric_datum_builder = MetricDatum::builder().dimensions(
            Dimension::builder()
                .name("ServiceName")
                .value(&self.config.service_name)
                .build(),
        );
        for (tag, value) in &self.tags {
            metric_datum_builder = metric_datum_builder
                .dimensions(Dimension::builder().name(tag).value(value).build());
        }
        metric_datum_builder = metric_datum_builder
            .timestamp(measurement.timestamp.into())
            .unit(StandardUnit::Percent);

        request_builder = request_builder.metric_data(
            metric_datum_builder
                .clone()
                .metric_name("CPUUtilization")
                .value(measurement.cpu_utilization)
                .build(),
        );
        request_builder = request_builder.metric_data(
            metric_datum_builder
                .clone()
                .metric_name("MemoryUtilization")
                .value(measurement.mem_utilization)
                .build(),
        );
        request_builder = request_builder.metric_data(
            metric_datum_builder
                .clone()
                .metric_name("MaxMemoryUtilization")
                .value(measurement.max_mem_utilization)
                .build(),
        );
        request_builder.send().await.map(|_| ()).map_err(Into::into)
    }
}
