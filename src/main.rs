mod cli;
mod config;
mod error;
mod provision;

use crate::cli::Cli;
use crate::error::AppResult;
use crate::provision::{provision_tool, Tool};
use clap::Parser;
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        // Using eprintln to ensure the error message is visible even if the UI is active.
        eprintln!("\n{} {}", style("Error:").red().bold(), e);
        std::process::exit(1);
    }
}

#[tracing::instrument]
async fn run() -> AppResult<()> {
    let cli = Cli::parse();

    // Conditionally initialize the tracing subscriber based on the verbose flag.
    if cli.verbose > 0 {
        let level = match cli.verbose {
            1 => "info",
            2 => "debug",
            _ => "trace",
        };
        let filter = format!("isoterm={}", level);
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_writer(std::io::stderr) // Write logs to stderr to not interfere with UI
            .init();
    }

    // Expand the user-provided path.
    let dest_dir_str = shellexpand::tilde(&cli.dest_dir).to_string();
    let env_dir = PathBuf::from(dest_dir_str);

    // Create environment directories.
    fs::create_dir_all(env_dir.join("bin"))?;
    fs::create_dir_all(env_dir.join("config"))?;
    fs::create_dir_all(env_dir.join("data"))?;

    // --- UI Setup ---
    let mp = MultiProgress::new();
    let spinner_style = ProgressStyle::with_template("{spinner:.green} {msg}")?
        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏-");

    mp.println(format!(
        "{} Setting up environment in {}",
        style("✓").green(),
        style(env_dir.display()).cyan()
    ))?;

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
            path_in_archive: Some("hx"),
        },
    ];

    // --- Provisioning Loop ---
    for tool in &tools {
        let pb = mp.add(ProgressBar::new_spinner());
        pb.enable_steady_tick(Duration::from_millis(120));
        pb.set_style(spinner_style.clone());
        provision_tool(&env_dir, tool, &pb, &spinner_style).await?;
    }

    // --- Configuration Step ---
    let pb_config = mp.add(ProgressBar::new_spinner());
    pb_config.enable_steady_tick(Duration::from_millis(120));
    pb_config.set_style(spinner_style.clone());
    config::generate_configs(&env_dir, &pb_config).await?;

    // Final success messages.
    mp.println(format!(
        "\n{} Environment setup complete!",
        style("🚀").green()
    ))?;
    mp.println("To activate your new shell environment, run:".to_string())?;
    mp.println(format!(
        "\n  source {}\n",
        env_dir.join("activate.sh").display()
    ))?;

    Ok(())
}