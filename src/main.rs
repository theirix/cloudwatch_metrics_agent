use cloudwatch_metrics_agent::config::CloudwatchConfig;
use cloudwatch_metrics_agent::main_runner;
use log::info;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
struct Opt {
    /// Metric namespace
    #[structopt(short, long)]
    namespace: String,

    /// Metric dimension value for ServiceName
    #[structopt(short, long)]
    service_name: String,

    /// Metric period
    #[structopt(short, long, default_value = "60")]
    period: u32,

    /// Whether to run without sending to CloudWatch
    #[structopt(short, long)]
    dryrun: bool,
}

#[tokio::main]
async fn main() -> Result<(), aws_sdk_cloudwatch::Error> {
    env_logger::Builder::from_default_env()
        .write_style(if atty::is(atty::Stream::Stdout) {
            env_logger::WriteStyle::Auto
        } else {
            env_logger::WriteStyle::Never
        })
        .init();

    let opt = Opt::from_args();
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
