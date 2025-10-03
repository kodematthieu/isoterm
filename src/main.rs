mod cli;
mod config;
mod error;
mod provision;

use crate::cli::Cli;
use crate::error::AppResult;
use crate::provision::{provision_tool, Tool};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::process::exit;

fn main() {
    if let Err(e) = run() {
        eprintln!("❌ Error: {:?}", e);
        exit(1);
    }
}

fn run() -> AppResult<()> {
    let cli = Cli::parse();

    // Expand the user-provided path (e.g., `~` to the home directory).
    let dest_dir_str = shellexpand::tilde(&cli.dest_dir).to_string();
    let env_dir = PathBuf::from(dest_dir_str);

    println!("Setting up environment in: {}", env_dir.display());

    // Create the main environment directory and its subdirectories.
    let bin_dir = env_dir.join("bin");
    let config_dir = env_dir.join("config");
    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&config_dir)?;

    println!("✅ Created environment directories.");

    // Define the list of tools to be provisioned.
    let tools = vec![
        Tool {
            name: "fish",
            repo: "fish-shell/fish-shell",
            binary_name: "fish",
        },
        Tool {
            name: "starship",
            repo: "starship/starship",
            binary_name: "starship",
        },
        Tool {
            name: "zoxide",
            repo: "ajeetdsouza/zoxide",
            binary_name: "zoxide",
        },
        Tool {
            name: "atuin",
            repo: "atuinsh/atuin",
            binary_name: "atuin",
        },
    ];

    // Provision each tool.
    for tool in &tools {
        provision_tool(&env_dir, tool)?;
    }

    // Generate all configuration files.
    config::generate_configs(&env_dir)?;

    println!("\n🚀 Environment setup complete!");
    println!("To activate your new shell environment, run:");
    println!("\n  source {}\n", env_dir.join("activate.sh").display());

    Ok(())
}