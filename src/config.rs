use clap::{Parser, Subcommand};
use std::sync::OnceLock;

/// WinCC Unified Debug Proxy - Proxies Chrome DevTools Protocol connections
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Proxies CDP connections to WinCC Unified for VS Code debugging",
    long_about = r#"Proxies Chrome DevTools Protocol (CDP) connections to WinCC Unified runtime,
enabling VS Code debugging with automatic reconnection when scripts reload.

The proxy monitors the WinCC debug server for target changes and automatically
restarts connections, forcing VS Code to reconnect without manual intervention.

EXAMPLES:
  ./wincc-unified-debug-proxy.exe                            Start proxy (localhost:9222)
  ./wincc-unified-debug-proxy.exe run -t 192.168.1.100       Connect to remote WinCC
  ./wincc-unified-debug-proxy.exe init                       Create .vscode/launch.json
  ./wincc-unified-debug-proxy.exe generate -a 192.168.1.100  Generate netsh .bat scripts for remote setup
  ./wincc-unified-debug-proxy.exe run --dump ./output --styleguide v19    Dump scripts + write styleguide"#
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize .vscode/launch.json for debugging
    Init {
        /// Output directory (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        output: String,

        /// Port for Dynamics proxy (used in launch.json)
        #[arg(short = 'd', long, default_value_t = 9230)]
        dynamics_port: u16,

        /// Port for Events proxy (used in launch.json)
        #[arg(short = 'e', long, default_value_t = 9231)]
        events_port: u16,
    },

    /// Generate .bat scripts for remote WinCC debugging (netsh port forwarding + firewall)
    Generate {
        /// IP address of the WinCC machine
        #[arg(short = 'a', long)]
        address: String,

        /// WinCC debug port
        #[arg(short = 'p', long, default_value_t = 9222)]
        port: u16,

        /// Output directory for .bat files (defaults to current directory)
        #[arg(short, long, default_value = ".")]
        output: String,
    },

    /// Start the debug proxy server (default command)
    #[command(name = "run")]
    Run {
        /// Target WinCC host address
        #[arg(short = 't', long, default_value = "localhost")]
        target_host: String,

        /// Target WinCC debug port
        #[arg(short = 'p', long, default_value_t = 9222)]
        target_port: u16,

        /// Local port for Dynamics proxy
        #[arg(short = 'd', long, default_value_t = 9230)]
        dynamics_port: u16,

        /// Local port for Events proxy
        #[arg(short = 'e', long, default_value_t = 9231)]
        events_port: u16,

        /// Poll interval in seconds
        #[arg(short = 'i', long, default_value_t = 1)]
        poll_interval: u64,

        /// Enable verbose logging
        #[arg(short = 'v', long)]
        verbose: bool,

        /// Enable very verbose logging
        #[arg(short = 'V', long)]
        very_verbose: bool,

        /// Show full (long) script paths instead of shortened ones
        #[arg(short = 'l', long)]
        long_paths: bool,

        /// Continuously dump runtime scripts to local files as they are loaded
        #[arg(long, default_value = None)]
        dump: Option<String>,

        /// Write styleguide files (.d.ts, .eslintrc.json, etc.) into the dump directory (v17, v18, v19, v20, v21)
        #[arg(long, default_value = None)]
        styleguide: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Configuration {
    pub target_host: String,
    pub target_port: u16,
    pub dynamics_port: u16,
    pub events_port: u16,
    pub poll_interval: u64,
    pub verbose: bool,
    pub very_verbose: bool,
    pub long_paths: bool,
    pub dump_output: Option<String>,
    pub styleguide_version: Option<String>,
}

impl Configuration {
    pub fn from_run_command(
        target_host: String,
        target_port: u16,
        dynamics_port: u16,
        events_port: u16,
        poll_interval: u64,
        verbose: bool,
        very_verbose: bool,
        long_paths: bool,
        dump_output: Option<String>,
        styleguide_version: Option<String>,
    ) -> Self {
        Self {
            target_host,
            target_port,
            dynamics_port,
            events_port,
            poll_interval,
            verbose,
            very_verbose,
            long_paths,
            dump_output,
            styleguide_version,
        }
    }

    pub fn default() -> Self {
        Self {
            target_host: "localhost".to_string(),
            target_port: 9222,
            dynamics_port: 9230,
            events_port: 9231,
            poll_interval: 5,
            verbose: false,
            very_verbose: false,
            long_paths: false,
            dump_output: None,
            styleguide_version: None,
        }
    }
}

pub static CONFIG: OnceLock<Configuration> = OnceLock::new();

pub fn config() -> &'static Configuration {
    CONFIG.get().expect("Configuration not initialized")
}
