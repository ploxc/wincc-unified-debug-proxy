mod commands;
mod config;
mod logging;
mod proxy;
mod styleguide;

use clap::Parser;
use config::{Cli, Commands, Configuration, CONFIG};

fn has_node() -> bool {
    std::process::Command::new("cmd")
        .args(["/C", "node", "--version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn prompt_styleguide_version() -> Option<String> {
    use std::io::{self, Write};

    if !has_node() {
        println!();
        println!("  Node.js is not installed (or not in PATH).");
        println!("  ESLint and IntelliSense require Node.js.");
        println!("  Install it from https://nodejs.org/ and restart.");
        println!();
        println!("  Continuing without styleguide...");
        println!();
        return None;
    }

    println!();
    println!("Which TIA Portal / WinCC Unified version are you using?");
    println!("  1) v17");
    println!("  2) v18");
    println!("  3) v19");
    println!("  4) v20");
    println!("  5) v21");
    println!("  s) Skip (no styleguide)");
    println!();
    print!("Version [1-5/s]: ");
    io::stdout().flush().ok();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let input = input.trim().to_lowercase();

    match input.as_str() {
        "1" | "v17" => Some("v17".to_string()),
        "2" | "v18" => Some("v18".to_string()),
        "3" | "v19" => Some("v19".to_string()),
        "4" | "v20" => Some("v20".to_string()),
        "5" | "v21" => Some("v21".to_string()),
        _ => None,
    }
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init {
            output,
            dynamics_port,
            events_port,
        }) => {
            if let Err(e) = commands::init_vscode(&output, dynamics_port, events_port) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Generate {
            address,
            port,
            output,
        }) => {
            if let Err(e) = commands::generate_netsh_scripts(&address, port, &output) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Run {
            target_host,
            target_port,
            dynamics_port,
            events_port,
            poll_interval,
            verbose,
            very_verbose,
            long_paths,
            dump,
        }) => {
            let styleguide_version = if dump.is_some() {
                prompt_styleguide_version()
            } else {
                None
            };

            let cfg = Configuration::from_run_command(
                target_host,
                target_port,
                dynamics_port,
                events_port,
                poll_interval,
                verbose,
                very_verbose,
                long_paths,
                dump,
                styleguide_version,
            );
            CONFIG.set(cfg).expect("Failed to set configuration");
        }
        None => {
            CONFIG
                .set(Configuration::default())
                .expect("Failed to set configuration");
        }
    }

    proxy::run_proxy().await;
}
