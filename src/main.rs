mod cli;
mod config;
mod error;
mod provision;

use crate::cli::Cli;
use crate::error::AppResult;
use crate::provision::Tool;
use anyhow::Context;
use clap::Parser;
use console::style;
use futures::future::try_join_all;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle, ProgressDrawTarget};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::task::JoinHandle;

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
        // This structure provides a default log level for all crates
        // and adjusts the level for `isoterm` based on verbosity.
        // -v: info
        // -vv: info, with isoterm at debug
        // -vvv: debug, with isoterm at trace
        // -vvvv: trace for everything
        let filter = match cli.verbose {
            1 => "info",
            2 => "info,isoterm=debug",
            3 => "debug,isoterm=trace",
            _ => "trace",
        };
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
            .with_ansi(console::user_attended())
            .with_writer(std::io::stderr) // Write logs to stderr to not interfere with UI
            .init();
    }

    run_inner(cli).await
}

async fn run_inner(cli: Cli) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .user_agent("isoterm")
        .build()
        .context("Failed to build reqwest client")?;

    // Expand the user-provided path.
    let dest_dir_str = shellexpand::tilde(&cli.dest_dir).to_string();
    let env_dir = PathBuf::from(dest_dir_str);

    // --- UI Setup ---
    // Conditionally hide the progress bars if not in a TTY. This is crucial
    // for CI environments and for ensuring that logs are not overwritten.
    let draw_target = if console::user_attended() {
        ProgressDrawTarget::stderr()
    } else {
        ProgressDrawTarget::hidden()
    };
    let mp = MultiProgress::with_draw_target(draw_target);
    let spinner_style =
        ProgressStyle::with_template("{spinner:.green} {msg}")?.tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏-");

    mp.println(format!(
        "{} Setting up environment in {}",
        style("✓").green(),
        style(env_dir.display()).cyan()
    ))?;

    // Emit logs after the initial UI setup to prevent them from being overwritten.
    tracing::trace!("trace log from run_inner");
    tracing::debug!("debug log from run_inner");
    tracing::info!("info log from run_inner");

    // Define the list of tools to be provisioned.
    let tools = vec![
        Tool {
            name: "fish",
            repo: "fish-shell/fish-shell",
            binary_name: "fish",
            path_in_archive: None,
            needs_source_share: true,
            version_arg: None,
        },
        Tool {
            name: "starship",
            repo: "starship/starship",
            binary_name: "starship",
            path_in_archive: None,
            needs_source_share: false,
            version_arg: None,
        },
        Tool {
            name: "zoxide",
            repo: "ajeetdsouza/zoxide",
            binary_name: "zoxide",
            path_in_archive: None,
            needs_source_share: false,
            version_arg: None,
        },
        Tool {
            name: "atuin",
            repo: "atuinsh/atuin",
            binary_name: "atuin",
            path_in_archive: None,
            needs_source_share: false,
            version_arg: None,
        },
        Tool {
            name: "ripgrep",
            repo: "BurntSushi/ripgrep",
            binary_name: "rg",
            path_in_archive: None,
            needs_source_share: false,
            version_arg: None,
        },
        Tool {
            name: "helix",
            repo: "helix-editor/helix",
            binary_name: "hx",
            path_in_archive: Some("hx"),
            needs_source_share: false,
            version_arg: Some("--version"),
        },
    ];

    // --- Main Transactional Block ---
    if let Err(e) = setup_environment(&env_dir, &tools, &mp, &spinner_style, &client).await {
        eprintln!(
            "\n{} {}",
            style("Fatal:").red().bold(),
            style(&e).red().bold()
        );
        eprintln!(
            "{}",
            style("Cleaning up partially created environment...").yellow()
        );
        fs::remove_dir_all(&env_dir)
            .context("Failed to clean up environment directory during error recovery")?;
        eprintln!("{}", style("Cleanup complete.").green());
        // Propagate the original error to exit with a non-zero status code.
        return Err(e);
    }

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

/// Encapsulates the entire environment setup process. If any step fails,
/// it returns an error, allowing the caller to perform a cleanup.
#[tracing::instrument(skip(tools, mp, spinner_style), fields(env_dir = %env_dir.display()))]
async fn setup_environment(
    env_dir: &Path,
    tools: &[Tool],
    mp: &MultiProgress,
    spinner_style: &ProgressStyle,
    client: &reqwest::Client,
) -> AppResult<()> {
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

    // --- Step 1: Create all UI spinners upfront ---
    let progress_bars: Vec<_> = tools
        .iter()
        .map(|tool| {
            let pb = mp.add(ProgressBar::new_spinner());
            pb.enable_steady_tick(Duration::from_millis(120));
            pb.set_style(spinner_style.clone());
            pb.set_message(format!("Queued {}...", style(tool.name).bold()));
            pb
        })
        .collect();

    // --- Step 2: Spawn all provisioning tasks ---
    let mut tasks: Vec<JoinHandle<AppResult<()>>> = Vec::new();
    for (tool, pb) in tools.iter().cloned().zip(progress_bars.into_iter()) {
        let env_dir = env_dir.to_path_buf();
        let spinner_style = spinner_style.clone();
        let client = client.clone();

        let task = tokio::spawn(async move {
            provision::provision_tool(&env_dir, &tool, &pb, &spinner_style, &client)
                .await
                .with_context(|| format!("Failed to provision tool: '{}'", tool.name))
        });
        tasks.push(task);
    }

    // --- Step 3: Await tasks concurrently. Abort all on first error. ---
    let results = try_join_all(tasks)
        .await
        .context("A provisioning task panicked or was cancelled")?;

    // Check for any application-level errors from the completed tasks
    for result in results {
        result.context("A provisioning task returned an error")?;
    }

    // --- Configuration Step (same as before) ---
    let pb_config = mp.add(ProgressBar::new_spinner());
    pb_config.enable_steady_tick(Duration::from_millis(120));
    pb_config.set_style(spinner_style.clone());
    config::generate_configs(env_dir, &pb_config)
        .await
        .context("Failed to generate configuration files")?;

    Ok(())
}
