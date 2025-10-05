use crate::error::AppResult;
use anyhow::{Context, anyhow};
use console::style;
use indicatif::ProgressBar;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Generates all necessary configuration files and the activation script.
#[tracing::instrument(skip(pb), fields(env_dir = %env_dir.display()))]
pub async fn generate_configs(env_dir: &Path, pb: &ProgressBar) -> AppResult<()> {
    pb.set_message("Generating configuration files...");

    let config_dir = env_dir.join("config");
    fs::create_dir_all(&config_dir).context("Failed to create config directory")?;

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

/// Creates the main `activate.sh` script for the environment.
#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_activate_script(env_dir: &Path) -> AppResult<()> {
    let script_content = include_str!("../templates/activate.sh");
    let script_path = env_dir.join("activate.sh");
    tracing::trace!(path = %script_path.display(), "Writing activate script");
    fs::write(&script_path, script_content).context("Failed to write activate.sh")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

/// Creates the `config.fish` file with initialization commands.
#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_fish_config(env_dir: &Path) -> AppResult<()> {
    let fish_config_dir = env_dir.join("config").join("fish");
    fs::create_dir_all(&fish_config_dir).context("Failed to create fish config directory")?;

    let config_content = include_str!("../templates/config.fish");

    let config_path = fish_config_dir.join("config.fish");
    tracing::trace!(path = %config_path.display(), "Writing fish config");
    fs::write(&config_path, config_content).context("Failed to write config.fish")?;
    Ok(())
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
    let atuin_data_dir = env_dir.join("data").join("atuin");
    fs::create_dir_all(&atuin_data_dir).context("Failed to create atuin data directory")?;

    let db_path = atuin_data_dir.join("history.db");
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow!("Invalid non-UTF8 path for atuin database"))?;

    let template_content = include_str!("../templates/atuin/config.toml.template");
    let config_content = template_content.replace("${DB_PATH}$", &db_path_str.replace('\\', "/"));

    let atuin_config_dir = env_dir.join("config").join("atuin");
    fs::create_dir_all(&atuin_config_dir)?;
    let config_path = atuin_config_dir.join("config.toml");
    tracing::trace!(path = %config_path.display(), "Writing atuin config");
    fs::write(&config_path, config_content).context("Failed to write atuin/config.toml")?;

    Ok(())
}

#[tracing::instrument(fields(env_dir = %env_dir.display()))]
fn write_helix_config(env_dir: &Path) -> AppResult<()> {
    let helix_config_dir = env_dir.join("config").join("helix");
    fs::create_dir_all(&helix_config_dir).context("Failed to create helix config directory")?;

    let config_toml_path = helix_config_dir.join("config.toml");
    let config_toml_content = include_str!("../templates/helix/config.toml");
    tracing::trace!(path = %config_toml_path.display(), "Writing helix config.toml");
    fs::write(&config_toml_path, config_toml_content)
        .context("Failed to write helix/config.toml")?;

    let languages_toml_path = helix_config_dir.join("languages.toml");
    let languages_toml_content = include_str!("../templates/helix/languages.toml");
    tracing::trace!(path = %languages_toml_path.display(), "Writing helix languages.toml");
    fs::write(&languages_toml_path, languages_toml_content)
        .context("Failed to write helix/languages.toml")?;

    Ok(())
}
