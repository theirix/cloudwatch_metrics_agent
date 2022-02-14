# Cloudwatch Metrics Agent

[![Crates.io](https://img.shields.io/crates/v/cloudwatch_metrics_agent.svg)](https://crates.io/crates/cloudwatch_metrics_agent)
[![Build](https://github.com/theirix/cloudwatch_metrics_agent/actions/workflows/build.yml/badge.svg)](https://github.com/theirix/cloudwatch_metrics_agent/actions/workflows/build.yml)

**cloudwatch_metrics_agent** is a simple agent for sending custom CPU and memory metrics to Cloudwatch.

## Installation

    cargo install cloudwatch_metrics_agent

## Usage

## Simple

To launch agent and send metrics to CloudWatch each minute:

    cloudwatch_metrics_agent --namespace TestNamespace --service FooService --period 60

To preview metrics without actual sending them to CloudWatch:

    RUST_LOG=info cloudwatch_metrics_agent --namespace TestNamespace --service FooService --period 60 --dry-run

## Agent in sidecar container

To deploy agent in ECS with Fargate or EC2 just add a container with the agent to a task definition with a monitored service.  Agent's container shares resources and namespace with other containers in a task definition so collected metrics are valid.

## Agent inside Docker container

If compute environment does not allow multi-container configurations (such as AWS Batch) it is possible to launch agent in background while keeping main process in foreground. To properly handle termination a supervisor or a init multiplexer like [tini](https://github.com/krallin/tini) or [dumb-init](https://github.com/Yelp/dumb-init) must be used so the backround agent process receive `SIGTERM` signal and flushes remaining metrics at shutdown. Because entrypoint shell script is used as a entrypoint, the whole process group must receive a signal. To achieve this the `tini` must be used a with a [process group `-g` flag](https://github.com/krallin/tini#process-group-killing) while the `dumb-init` does it by default.
The entrypoint script must also send metrics on succesfull foreground process termination when no signals sent.

Example entrypoint script:
```sh
#!/bin/bash

# launch agent in the background and save its pid
cloudwatch_metrics_agent --namespace TestNamespace --service FooService &
child=$!

# launch main process in the foreground
"$@"
exitcode=$?

echo "Terminating agent after foreground process exit" >&2
kill -TERM "$child"
wait "$child"
echo "Exiting with $exitcode" >&2
exit $exitcode
```

Example `ENTRYPOINT` directive for `Dockerfile`:
```dockerfile
ENTRYPOINT ["tini", "-g", "--"]
```
or
```dockerfile
ENTRYPOINT ["dumb-init"]
```



# How does it work

The agent collects system metrics (CPU and memory utilization) and periodically sends them to a publisher.  Publisher can be a AWS CloudWatch service or console (for debugging).

Period (`--period` parameter) specifies how often logs are emitted to publisher, one minute by default. Metrics are collected during this period with much higher resolution (0.9 seconds) and aggregated by median and max values. So it relatively safe to set a publishing period to five minutes (non-detailed CloudWatch metrics default).

If the cloud service is unavailable while sending a bunch of metrics after two retries, this bunch is skipped.
When the agent is stopped (SIGTERM or SIGINT signal received), all remaining metrics are flushed to the publisher.

Emitted metrics:

- `CPUUtilization` - median CPU utilization across all CPU cores, in percents.

- `MemoryUtilization` - median memory utilization, in percents. Calculated as used memory divided by total memory in percents where used memory is total memory without free, buffers, page cache and slabs.

Metrics are published to the specified namespace and service name. AWS credentials [are configured](https://docs.aws.amazon.com/sdk-for-rust/latest/dg/credentials.html) via the environment variables, config files or IAM role.

Logs are configured via [env_logger](https://crates.io/crates/env_logger) so the `RUST_LOG` environment variable controls logging
with errors only by default. To show only emitted metrics while omitting other logs set the environment variable `RUST_LOG="cloudwatch_metrics_agent::publisher=info"` or `RUST_LOG="cloudwatch_metrics_agent::cloudwatch=info"`.

The agent uses Tokio framework for asynchronous work, AWS SDK for Rust for communicating with AWS and `sysinfo` for gathering metrics.

## Portability

Works on Linux and macOS.

To build a static binary for usage in container use a static CRT linkage:

    cargo build --release --target x86_64-unknown-linux-musl


## License

BSD 3-Clause
