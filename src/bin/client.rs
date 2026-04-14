use anstyle::AnsiColor;
use clap::builder::styling::Styles;
use clap::{Parser, Subcommand};

use tftp::{SessionConfig, TftpClient};

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default())
    .usage(AnsiColor::Green.on_default())
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::Red.on_default());

#[derive(Parser, Debug)]
#[command(about = "A high-performance asynchronous TFTP client")]
#[command(styles = STYLES)]
pub struct Cli {
    /// Server address
    #[arg(short, long, default_value = "127.0.0.1:69")]
    pub addr: std::net::SocketAddr,

    /// Local directory
    #[arg(short, long, default_value = ".")]
    pub directory: std::path::PathBuf,

    /// Timeout (ms)
    #[arg(short, long, default_value_t = 1000)]
    pub timeout: u64,

    /// Max retries
    #[arg(short, long, default_value_t = 3)]
    pub retry: u8,

    /// Block size
    #[arg(short, long, default_value_t = 512)]
    pub blksize: u16,

    /// Window size
    #[arg(short, long, default_value_t = 1)]
    pub windowsize: u16,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Download a file from TFTP server
    Get {
        /// Remote filename
        filename: String,
    },
    /// Upload a file to TFTP server
    Put {
        /// Local filename
        filename: String,
    },
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Cli::parse();
    let config = SessionConfig {
        directory: args.directory,
        timeout: args.timeout,
        retry: args.retry,
        gbn: false,
    };
    let client = TftpClient::new(config, args.blksize, args.windowsize);

    let result = match args.command {
        Command::Get { filename } => client.get_file(args.addr, filename).await,
        Command::Put { filename } => client.put_file(args.addr, filename).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
