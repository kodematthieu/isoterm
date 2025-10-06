use crate::error::AppResult;
use anyhow::{anyhow, Context};
use console::style;
use indicatif::ProgressBar;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Generates all necessary configuration files and the activation script.
#[tracing::instrument(skip(pb), fields(env_dir = %env_dir.display()))]
pub async fn generate_configs(env_dir: &Path, pb: &ProgressBar) -> AppResult<()> {
    pb.set_message("Generating configuration files...");

    // Generate activate.sh
    write_activate_script(env_dir)?;

    // Generate fish config
    write_fish_config(env_dir)?;

    // Generate starship config
    write_starship_config(env_dir)?;

    // Generate atuin config
    write_atuin_config(env_dir)?;

    // Generate helix config
    write_helix_config(env_dir)?;

    pb.finish_with_message(format!(
        "{} Generated configuration files",
        style("✓").green()
    ));
    Ok(())
}

/// A helper to write a config file, creating parent directories if they don't exist.
fn write_config_file(
    env_dir: &Path,
    relative_path: &str,
    content: &str,
) -> AppResult<()> {
    let final_path = env_dir.join(relative_path);
    if let Some(parent_dir) = final_path.parent() {
        fs::create_dir_all(parent_dir).with_context(|| {
            format!(
                "Failed to create parent directory for {}",
                final_path.display()
            )
        })?;
    }
    tracing::trace!(path = %final_path.display(), "Writing config file");
    fs::write(&final_path, content).with_context(|| format!("Failed to write {}", final_path.display()))
}

/// Creates the main `activate.sh` script for the environment.
#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_activate_script(env_dir: &Path) -> AppResult<()> {
    let script_content = include_str!("../templates/activate.sh");
    write_config_file(env_dir, "activate.sh", script_content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let script_path = env_dir.join("activate.sh");
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

/// Creates the `config.fish` file with initialization commands.
#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_fish_config(env_dir: &Path) -> AppResult<()> {
    let config_content = include_str!("../templates/config.fish");
    write_config_file(
        env_dir,
        "config/fish/config.fish",
        config_content,
    )
}

/// Creates a default `starship.toml` configuration using `starship preset`.
#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_starship_config(env_dir: &Path) -> AppResult<()> {
    let config_path = env_dir.join("config").join("starship.toml");
    let starship_bin = env_dir.join("bin").join("starship");

    tracing::trace!(path = %config_path.display(), "Generating starship config");

    let status = Command::new(&starship_bin)
        .arg("preset")
        .arg("no-empty-icons")
        .arg("-o")
        .arg(&config_path)
        .status()
        .context("Failed to execute starship preset command")?;

    if !status.success() {
        return Err(anyhow!(
            "starship preset command failed with status: {}",
            status
        ));
    }

    Ok(())
}

/// Creates a default `atuin/config.toml` configuration.
#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_atuin_config(env_dir: &Path) -> AppResult<()> {
    // Ensure the data directory exists first.
    let atuin_data_dir = env_dir.join("data").join("atuin");
    fs::create_dir_all(&atuin_data_dir).context("Failed to create atuin data directory")?;

    let db_path = atuin_data_dir.join("history.db");
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid non-UTF8 path for atuin database"))?;

    let template_content = include_str!("../templates/atuin/config.toml.template");
    let config_content = template_content.replace("${DB_PATH}$", &db_path_str.replace('\\', "/"));

    write_config_file(
        env_dir,
        "config/atuin/config.toml",
        &config_content,
    )
}

#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_helix_config(env_dir: &Path) -> AppResult<()> {
    let config_toml_content = include_str!("../templates/helix/config.toml");
    write_config_file(
        env_dir,
        "config/helix/config.toml",
        config_toml_content,
    )?;

    let languages_toml_content = include_str!("../templates/helix/languages.toml");
    write_config_file(
        env_dir,
        "config/helix/languages.toml",
        languages_toml_content,
    )
}
