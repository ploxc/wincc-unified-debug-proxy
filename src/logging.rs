use chrono::Local;
use colored::Colorize;

use crate::config::config;

pub fn timestamp() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub fn log(message: &str) {
    println!(
        "{} {}",
        format!("[{}]", timestamp()).dimmed(),
        message
    );
}

pub fn log_success(message: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", timestamp()).dimmed(),
        "[OK]".green().bold(),
        message
    );
}

pub fn log_warn(message: &str) {
    println!(
        "{} {} {}",
        format!("[{}]", timestamp()).dimmed(),
        "[WARN]".yellow().bold(),
        message
    );
}

pub fn log_error(message: &str) {
    eprintln!(
        "{} {} {}",
        format!("[{}]", timestamp()).dimmed(),
        "[ERROR]".red().bold(),
        message
    );
}

pub fn log_verbose(message: &str) {
    if config().verbose {
        println!(
            "{} {} {}",
            format!("[{}]", timestamp()).dimmed(),
            "[VERBOSE]".dimmed(),
            message.dimmed()
        );
    }
}

pub fn log_very_verbose(message: &str) {
    if config().very_verbose {
        println!(
            "{} {} {}",
            format!("[{}]", timestamp()).dimmed(),
            "[VERBOSE]".dimmed(),
            message.dimmed()
        );
    }
}
