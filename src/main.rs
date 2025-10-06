mod cli;
mod config;
mod error;
mod provision;

use crate::{
    cli::Cli,
    error::AppResult,
    provision::{
        ProvisionContext, atuin::Atuin, fish::Fish, helix::Helix, provision_tool, ripgrep::Ripgrep,
        starship::Starship, zoxide::Zoxide,
    },
};
use anyhow::Context;
use clap::Parser;
use console::style;
use futures::future::try_join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        if let Some(user_error) = e.downcast_ref::<error::UserError>() {
            // It's a known, user-facing error. Print it cleanly.
            eprintln!("\n{} {}", style("Error:").red().bold(), user_error);
        } else {
            // It's an unexpected internal error. Print the full context for debugging.
            eprintln!(
                "\n{} An unexpected error occurred.",
                style("Fatal:").red().bold()
            );
            eprintln!("{:?}", e);
        }
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
    let setup_result: AppResult<()> = (|| async {
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

        // --- Create the configuration overlay ---
        config::symlink_unmanaged_configs(&env_dir)?;
        tracing::info!("Created symlink overlay for unmanaged configurations");

        // --- Overall Progress Bar ---
        let tools_to_provision = ["fish", "starship", "zoxide", "atuin", "ripgrep", "helix"];
        let total_steps = (tools_to_provision.len() + 1) as u64; // Tools + config step

        let overall_pb = mp.add(ProgressBar::new(total_steps));
        let overall_style = ProgressStyle::with_template("[{pos}/{len}] {wide_msg}")?;
        overall_pb.set_style(overall_style);
        overall_pb.set_message("Initializing...");
        let overall_pb = Arc::new(overall_pb);

        // --- Spawn all provisioning tasks ---
        let context = ProvisionContext {
            env_dir: env_dir.clone(),
            client,
        };

        let tasks = vec![
            tokio::spawn(provision_tool(
                Fish,
                context.clone(),
                mp.clone(),
                overall_pb.clone(),
            )),
            tokio::spawn(provision_tool(
                Starship,
                context.clone(),
                mp.clone(),
                overall_pb.clone(),
            )),
            tokio::spawn(provision_tool(
                Zoxide,
                context.clone(),
                mp.clone(),
                overall_pb.clone(),
            )),
            tokio::spawn(provision_tool(
                Atuin,
                context.clone(),
                mp.clone(),
                overall_pb.clone(),
            )),
            tokio::spawn(provision_tool(
                Ripgrep,
                context.clone(),
                mp.clone(),
                overall_pb.clone(),
            )),
            tokio::spawn(provision_tool(
                Helix,
                context.clone(),
                mp.clone(),
                overall_pb.clone(),
            )),
        ];

        // --- Await tasks concurrently ---
        let results = try_join_all(tasks)
            .await
            .context("A provisioning task panicked or was cancelled")?;
        for result in results {
            result.context("A provisioning task returned an error")?;
        }

        // --- Configuration Step ---
        overall_pb.set_message("Generating configuration files...");
        config::generate_configs(&env_dir, &overall_pb).await?;
        overall_pb.println(format!(
            "{} Generated configuration files",
            style("✓").green()
        ));
        overall_pb.inc(1);

        // --- Finalization ---
        overall_pb.finish_and_clear();
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
        if let Some(user_error) = e.downcast_ref::<error::UserError>() {
            // It's a known, user-facing error. Print it cleanly.
            eprintln!("\n{} {}", style("Error:").red().bold(), user_error);
        } else {
            // It's an unexpected internal error. Print the full context for debugging.
            eprintln!(
                "\n{} An unexpected error occurred.",
                style("Fatal:").red().bold()
            );
            eprintln!("{:?}", e);
        }
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
