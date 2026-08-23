use crate::config::CloudwatchConfig;
use crate::metrics::Measurement;
use crate::publisher::MetricPublisher;

use async_trait::async_trait;
use aws_config::imds::client::Client as ImdsClient;
use aws_config::meta::region::RegionProviderChain;
use aws_config::{BehaviorVersion, SdkConfig};
use aws_sdk_cloudwatch::types::{Dimension, MetricDatum, StandardUnit};
use aws_sdk_cloudwatch::Client;
use log::{debug, info, warn};
use serde_json::Value;
use std::collections::HashMap;

/// Sink implementation that sends metrics to Cloudwatch
pub struct CloudwatchPublisher {
    client: Client,
    config: CloudwatchConfig,
    tags: HashMap<String, String>,
}

pub async fn create_cloudwatch_publisher(config: CloudwatchConfig) -> CloudwatchPublisher {
    let aws_config = get_aws_config().await;
    info!("Using custom tags: {:?}", config.tags);
    let mut tags: HashMap<String, String> = if config.publish_instance_id {
        match get_instance_id().await {
            Some(instance_id) => HashMap::from([("InstanceId".to_string(), instance_id)]),
            None => HashMap::new(),
        }
    } else {
        HashMap::new()
    };
    tags.extend(config.tags.clone());
    info!("Using tags: {:?}", tags);
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
    info!("Get instance-id: {}", instance_id);
    Ok(instance_id)
}

fn ecs_metadata_uri() -> Option<String> {
    std::env::var("ECS_CONTAINER_METADATA_URI_V4")
        .ok()
        .map(|value| value + "/task")
}

async fn get_fargate_instance_id() -> Result<String, Box<dyn std::error::Error>> {
    get_fargate_instance_id_from(
        ecs_metadata_uri()
            .ok_or_else(|| "No ECS_CONTAINER_METADATA_URI_V4 env var found".to_string())?,
    )
    .await
}

async fn get_fargate_instance_id_from(url: String) -> Result<String, Box<dyn std::error::Error>> {
    let http_client = reqwest::Client::new();
    let response = http_client
        .get(&url)
        .send()
        .await
        .map_err(|err| format!("Failed to make request to {}: {}", url, err))?;
    if !response.status().is_success() {
        return Err(Box::from(
            format!("Unexpected response code={}", response.status()).to_string(),
        ));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|err| format!("No json {}", err).to_string())?;
    debug!("Fargate task metadata: {}", body);
    // get TaskARN from body json and then extract taskid
    let instance_id: String = body
        .get("TaskARN")
        .and_then(|task_arn| task_arn.as_str())
        .ok_or_else(|| "No TaskARN found in Fargate metadata".to_string())?
        .split('/')
        .next_back()
        .ok_or_else(|| "No TaskARN found in Fargate metadata".to_string())?
        .to_string();
    info!("Get instance-id: {}", instance_id);
    Ok(instance_id)
}

async fn get_instance_id() -> Option<String> {
    match get_ec2_instance_id().await {
        Ok(instance_id) => Some(instance_id),
        Err(err) => {
            warn!("Cannot get EC2 instance id: {}", err);
            debug!("Trying Fargate");
            match get_fargate_instance_id().await {
                Ok(instance_id) => Some(instance_id),
                Err(err) => {
                    warn!("Cannot get Fargate instance id: {}", err);
                    None
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_fargate_instance_id() {
        let task_metadata = r#"{ "TaskARN": "arn:aws:ecs:us-east-1:account:task/airflow-cluster/8716a00d9f3e4ef8afe33bc6a3a9b393" } "#;
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("GET", "/task")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(task_metadata)
            .create();

        let instance_id = get_fargate_instance_id_from(server.url() + "/task").await;
        assert!(instance_id.is_ok());
        assert_eq!(instance_id.unwrap(), "8716a00d9f3e4ef8afe33bc6a3a9b393");
    }

    #[tokio::test]
    async fn test_get_fargate_instance_id_no_attribute() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("GET", "/task")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("{}")
            .create();

        let instance_id = get_fargate_instance_id_from(server.url() + "/task").await;
        assert!(instance_id.is_err());
    }

    #[tokio::test]
    async fn test_get_fargate_instance_id_not_found() {
        let mut server = mockito::Server::new_async().await;

        let _mock = server
            .mock("GET", "/task")
            .with_status(404)
            .with_header("content-type", "application/json")
            .create();

        let instance_id = get_fargate_instance_id_from(server.url() + "/task").await;
        assert!(instance_id.is_err());
    }
}
