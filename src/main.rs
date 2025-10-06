mod cli;
mod config;
mod error;
mod provision;

use crate::{
    cli::Cli,
    error::AppResult,
    provision::{
        atuin::Atuin, fish::Fish, helix::Helix, provision_tool, ripgrep::Ripgrep,
        starship::Starship, zoxide::Zoxide, ProvisionContext,
    },
};
use anyhow::Context;
use clap::Parser;
use console::style;
use futures::future::try_join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
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

async fn run() -> AppResult<()> {
    let cli = Cli::parse();

    // Conditionally initialize the tracing subscriber based on the verbose flag.
    if cli.verbose > 0 {
        let filter = match cli.verbose {
            1 => "info",
            2 => "info,isoterm=debug",
            3 => "debug,isoterm=trace",
            _ => "trace",
        };
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_ansi(console::user_attended())
            .with_writer(std::io::stderr)
            .init();
    }

    let env_dir = PathBuf::from(shellexpand::tilde(&cli.dest_dir).to_string());

    // The entire setup is wrapped in a closure that returns a Result.
    // This allows us to handle any error gracefully by cleaning up the environment directory.
    let setup_result = (|| async {
        let client = reqwest::Client::builder()
            .user_agent("isoterm")
            .build()
            .context("Failed to build reqwest client")?;

        let draw_target = if console::user_attended() {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        };
        let mp = MultiProgress::with_draw_target(draw_target);
        let spinner_style = ProgressStyle::with_template("{spinner:.green} {msg}")?
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏-");

        mp.println(format!(
            "{} Setting up environment in {}",
            style("✓").green(),
            style(env_dir.display()).cyan()
        ))?;

        tracing::info!("Starting environment setup");

        // --- Create environment directories ---
        let bin_dir = env_dir.join("bin");
        fs::create_dir_all(&bin_dir)?;
        tracing::trace!(path = %bin_dir.display(), "Created bin directory");

        let config_dir = env_dir.join("config");
        fs::create_dir_all(&config_dir)?;
        tracing::trace!(path = %config_dir.display(), "Created config directory");

        let data_dir = env_dir.join("data");
        fs::create_dir_all(&data_dir)?;
        tracing::trace!(path = %data_dir.display(), "Created data directory");

        // --- Create Progress Bars ---
        let tool_progress_bars =
            ["fish", "starship", "zoxide", "atuin", "ripgrep", "helix"].map(|name| {
                let pb = mp.add(ProgressBar::new_spinner());
                pb.enable_steady_tick(Duration::from_millis(120));
                pb.set_style(spinner_style.clone());
                pb.set_message(format!("Queued {}...", style(name).bold()));
                pb
            });
        let [pb_fish, pb_starship, pb_zoxide, pb_atuin, pb_ripgrep, pb_helix] =
            tool_progress_bars;

        // --- Spawn all provisioning tasks ---
        let context = ProvisionContext {
            env_dir: env_dir.clone(),
            client,
        };

        let tasks = vec![
            tokio::spawn({
                let context = context.clone();
                let style = spinner_style.clone();
                async move { provision_tool(Fish, &context, &pb_fish, &style).await }
            }),
            tokio::spawn({
                let context = context.clone();
                let style = spinner_style.clone();
                async move { provision_tool(Starship, &context, &pb_starship, &style).await }
            }),
            tokio::spawn({
                let context = context.clone();
                let style = spinner_style.clone();
                async move { provision_tool(Zoxide, &context, &pb_zoxide, &style).await }
            }),
            tokio::spawn({
                let context = context.clone();
                let style = spinner_style.clone();
                async move { provision_tool(Atuin, &context, &pb_atuin, &style).await }
            }),
            tokio::spawn({
                let context = context.clone();
                let style = spinner_style.clone();
                async move { provision_tool(Ripgrep, &context, &pb_ripgrep, &style).await }
            }),
            tokio::spawn({
                let context = context.clone();
                let style = spinner_style.clone();
                async move { provision_tool(Helix, &context, &pb_helix, &style).await }
            }),
        ];

        // --- Await tasks concurrently ---
        let results = try_join_all(tasks)
            .await
            .context("A provisioning task panicked or was cancelled")?;
        for result in results {
            result.context("A provisioning task returned an error")?;
        }

        // --- Configuration Step ---
        let pb_config = mp.add(ProgressBar::new_spinner());
        pb_config.enable_steady_tick(Duration::from_millis(120));
        pb_config.set_style(spinner_style);
        config::generate_configs(&env_dir, &pb_config).await?;

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
    })()
    .await;

    // --- Transactional Cleanup ---
    if let Err(e) = setup_result {
        eprintln!("\n{} {}", style("Fatal:").red().bold(), style(&e).red());
        eprintln!(
            "{}",
            style("Cleaning up partially created environment...").yellow()
        );
        fs::remove_dir_all(&env_dir)
            .context("Failed to clean up environment directory during error recovery")?;
        eprintln!("{}", style("Cleanup complete.").green());
        return Err(e);
    }

    Ok(())
}