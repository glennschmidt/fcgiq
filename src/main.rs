mod config;
mod pool;
mod queue;
mod cli;
mod runner;
mod item;

use crate::cli::Args;
use crate::config::Config;
use crate::pool::Pool;
use crate::queue::Queue;
use crate::runner::Runner;
use anyhow::{anyhow, Context, Error};
use aws_config::BehaviorVersion;
use clap::Parser;
use log::LevelFilter;
use simple_logger::SimpleLogger;
use std::str::FromStr;
use std::sync::Arc;
use tokio::signal;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> Result<(), Error> {
    //Parse configuration
    let args = Args::parse();
    let config = Config::from_file(&args.config)
        .context("Configuration file error")?;
    let log_level = LevelFilter::from_str(&config.log_level)
        .context("Unrecognized value for log_Level in configuration file")?;
    let mut aws_config = aws_config::defaults(BehaviorVersion::v2024_03_28());
    if !config.queue.sqs.api_endpoint_url.is_empty() {
        aws_config = aws_config.endpoint_url(&config.queue.sqs.api_endpoint_url);
    }

    //Initialize components
    SimpleLogger::new().with_level(log_level).init()?;
    let queue = Arc::new(
        Queue::new(
            config.queue.sqs.queue_url.clone(),
            config.queue.sqs.visibility_timeout,
            &aws_config.load().await,
        )
    );
    let pool = Arc::new(
        Pool::new(
            config.fastcgi.address.clone(),
            config.fastcgi.port,
            config.fastcgi.script_path.clone(),
            config.fastcgi.cgi_environment.clone(),
        )
    );

    //Start runner
    log::info!("fcgiq v{} is starting", VERSION);
    let runner = Runner::start(
        config.fastcgi.max_parallel_requests as usize,
        Arc::clone(&pool),
        Arc::clone(&queue),
        config.field_mappings.clone(),
    );
    log::info!("Listening on queue {}", &config.queue.sqs.queue_url);

    //Wait for termination signal
    match signal::ctrl_c().await {
        Ok(()) => {
            log::info!("Received shutdown request.");
        },
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {:#}", anyhow!(err));
        },
    }
    runner.stop().await;
    Ok(())
}
