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
        tracing::error!(error = ?e, "Application failed");
        exit(1);
    }
}

#[tracing::instrument]
fn run() -> AppResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();

    // Expand the user-provided path (e.g., `~` to the home directory).
    let dest_dir_str = shellexpand::tilde(&cli.dest_dir).to_string();
    let env_dir = PathBuf::from(dest_dir_str);

    tracing::info!(path = %env_dir.display(), "Setting up environment");

    // Create the main environment directory and its subdirectories.
    let bin_dir = env_dir.join("bin");
    let config_dir = env_dir.join("config");
    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&config_dir)?;

    tracing::info!("Created environment directories.");

    // Define the list of tools to be provisioned.
    let tools = vec![
        Tool {
            name: "fish",
            repo: "fish-shell/fish-shell",
            binary_name: "fish",
            path_in_archive: Some("bin/fish"),
        },
        Tool {
            name: "starship",
            repo: "starship/starship",
            binary_name: "starship",
            path_in_archive: None,
        },
        Tool {
            name: "zoxide",
            repo: "ajeetdsouza/zoxide",
            binary_name: "zoxide",
            path_in_archive: None,
        },
        Tool {
            name: "atuin",
            repo: "atuinsh/atuin",
            binary_name: "atuin",
            path_in_archive: None,
        },
        Tool {
            name: "ripgrep",
            repo: "BurntSushi/ripgrep",
            binary_name: "rg",
            path_in_archive: None,
        },
        Tool {
            name: "helix",
            repo: "helix-editor/helix",
            binary_name: "hx",
            path_in_archive: Some("hx"), // The extraction logic will strip the top-level dir
        },
    ];

    // Provision each tool.
    for tool in &tools {
        provision_tool(&env_dir, tool)?;
    }

    // Generate all configuration files.
    config::generate_configs(&env_dir)?;

    tracing::info!("\n🚀 Environment setup complete!");
    tracing::info!("To activate your new shell environment, run:");
    tracing::info!("\n  source {}\n", env_dir.join("activate.sh").display());

    Ok(())
}