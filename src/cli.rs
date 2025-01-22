use anstyle::AnsiColor;
use clap::builder::styling::Styles;
use clap::Parser;
use std::net::IpAddr;
use std::path::PathBuf;

const STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default())
    .usage(AnsiColor::Green.on_default())
    .literal(AnsiColor::Cyan.on_default())
    .placeholder(AnsiColor::Red.on_default());

#[derive(Parser, Debug)]
#[command(name = "tftp")]
#[command(about = "A simple TFTP client/server", long_about = None)]
#[command(styles = STYLES)]
pub struct Cli {
    /// Listen ip
    #[arg(short, long, default_value = "0.0.0.0")]
    ip: IpAddr,

    /// Port
    #[arg(short, long, default_value_t = 69)]
    port: u16,

    /// Work directory
    #[arg(short, long, default_value = ".")]
    directory: PathBuf,
}
