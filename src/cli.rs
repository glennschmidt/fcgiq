use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Path to the configuration file
    #[arg(short = 'c', long, default_value = "config.yaml")]
    pub config: String,
}
