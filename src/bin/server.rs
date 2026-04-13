use anstyle::AnsiColor;
use clap::Parser;
use clap::builder::styling::Styles;

use tftp::{SessionConfig, TftpServer};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default())
    .usage(AnsiColor::Green.on_default())
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::Red.on_default());

#[derive(Parser, Debug)]
#[command(about = "A high-performance asynchronous TFTP server")]
#[command(styles = STYLES)]
pub struct Cli {
    /// Listen address
    #[arg(short, long, default_value = "0.0.0.0:69")]
    pub addr: std::net::SocketAddr,

    /// Work directory
    #[arg(short, long, default_value = ".")]
    pub directory: std::path::PathBuf,

    /// Timeout (ms)
    #[arg(short, long, default_value_t = 1000)]
    pub timeout: u64,

    /// Max retries
    #[arg(short, long, default_value_t = 3)]
    pub retry: u8,

    /// Enable GO-Back-N
    #[arg(short, long)]
    pub gbn: bool,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    let config = SessionConfig {
        directory: args.directory,
        timeout: args.timeout,
        retry: args.retry,
        gbn: args.gbn,
    };

    env_logger::init();

    let server = TftpServer::new(args.addr, config);
    server.run().await.unwrap();
}