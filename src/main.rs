use clap::Parser;
use cloudwatch_metrics_agent::config::CloudwatchConfig;
use cloudwatch_metrics_agent::main_runner;
use log::info;
use std::collections::HashMap;

#[derive(Debug, Parser)]
struct Opt {
    /// Metric namespace
    #[arg(short, long)]
    namespace: String,

    /// Metric dimension value for ServiceName
    #[arg(short, long)]
    service_name: String,

    /// Metric period
    #[arg(short, long, default_value_t = 60)]
    period: u32,

    /// Whether to run without sending to CloudWatch
    #[arg(short, long)]
    dryrun: bool,

    /// Custom tags in format '--tags tag1 value1 --tags tag2 value2'
    #[arg(long, value_names = ["tag", "value"], num_args = 2, action = clap::ArgAction::Append)]
    tags: Vec<String>,
}

#[tokio::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), aws_sdk_cloudwatch::Error> {
    env_logger::Builder::from_default_env().init();

    let opt = Opt::parse();
    let tags: HashMap<String, String> = opt
        .tags
        .chunks_exact(2)
        .map(|pair| (pair[0].clone(), pair[1].clone()))
        .collect();
    let cloudwatch_config = CloudwatchConfig {
        namespace: opt.namespace,
        service_name: opt.service_name,
        tags,
    };

    main_runner(cloudwatch_config, opt.dryrun, opt.period).await?;

    info!("Done");
    Ok(())
}
