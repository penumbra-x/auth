mod cagen;
mod daemon;
mod error;
mod proxy;
mod serve;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use reqwest::Url;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Parser)]
#[clap(author, version, about, arg_required_else_help = true)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Opt {
    #[clap(subcommand)]
    pub commands: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run server
    Run(BootArgs),
    /// Start server daemon
    #[cfg(target_family = "unix")]
    Start(BootArgs),
    /// Restart server daemon
    #[cfg(target_family = "unix")]
    Restart(BootArgs),
    /// Stop server daemon
    #[cfg(target_family = "unix")]
    Stop,
    /// Show the server daemon log
    #[cfg(target_family = "unix")]
    Log,
    /// Show the server daemon process
    #[cfg(target_family = "unix")]
    PS,
}

#[derive(Args, Clone, Debug)]
pub struct BootArgs {
    /// Debug mode
    #[clap(short, long)]
    pub debug: bool,

    /// Bind address
    #[clap(short, long, default_value = "0.0.0.0:8000")]
    pub bind: SocketAddr,

    #[clap(short, long)]
    pub proxy: Option<Url>,

    /// MITM server CA certificate file path
    #[clap(long, default_value = "ca/cert.crt", requires = "bind")]
    pub cert: PathBuf,

    /// MITM server CA private key file path
    #[clap(long, default_value = "ca/key.pem", requires = "bind")]
    pub key: PathBuf,
}

fn main() -> Result<()> {
    let opt = Opt::parse();

    match opt.commands {
        Commands::Run(args) => daemon::run(args)?,
        #[cfg(target_family = "unix")]
        Commands::Start(args) => daemon::start(args)?,
        #[cfg(target_family = "unix")]
        Commands::Restart(args) => daemon::restart(args)?,
        #[cfg(target_family = "unix")]
        Commands::Stop => daemon::stop()?,
        #[cfg(target_family = "unix")]
        Commands::PS => daemon::status(),
        #[cfg(target_family = "unix")]
        Commands::Log => daemon::log()?,
    };

    Ok(())
}
