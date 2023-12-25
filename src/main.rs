use clap::Parser;
use cloudwatch_metrics_agent::config::CloudwatchConfig;
use cloudwatch_metrics_agent::main_runner;
use log::info;

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
}

#[tokio::main]
#[allow(clippy::result_large_err)]
async fn main() -> Result<(), aws_sdk_cloudwatch::Error> {
    env_logger::Builder::from_default_env().init();

    let opt = Opt::parse();
    let cloudwatch_config = CloudwatchConfig {
        namespace: opt.namespace,
        service_name: opt.service_name,
    };

    main_runner(cloudwatch_config, opt.dryrun, opt.period)
        .await
        .unwrap();

    info!("Done");
    Ok(())
}
