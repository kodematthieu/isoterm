use crate::error::AppResult;
use anyhow::Context;
use std::fs;
use std::path::Path;

/// Generates all necessary configuration files and the activation script.
pub fn generate_configs(env_dir: &Path) -> AppResult<()> {
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

    println!("✅ All configuration files generated successfully.");
    Ok(())
}

/// Creates the main `activate.sh` script for the environment.
fn write_activate_script(env_dir: &Path) -> AppResult<()> {
    let script_content = format!(
        r#"#!/bin/sh

# Get the absolute path to the environment directory
# THIS_SCRIPT is: {env_dir}/activate.sh
THIS_SCRIPT=$(readlink -f "$0" 2>/dev/null || realpath "$0" 2>/dev/null || (cd "$(dirname "$0")" && pwd -P)/$(basename "$0"))
ENV_DIR=$(dirname "$THIS_SCRIPT")

# Prepend our private bin directory to the PATH
export PATH="$ENV_DIR/bin:$PATH"

# Set environment variables to use our private configs
export STARSHIP_CONFIG="$ENV_DIR/config/starship.toml"
export ATUIN_CONFIG_DIR="$ENV_DIR/config/atuin"

# Set environment variable to tell fish where its runtime files are
export FISH_HOME="$ENV_DIR/fish_runtime"

# Execute fish shell
# The '-l' flag makes it a login shell
# The '-C' flag sets the initial command to source our config
exec "$ENV_DIR/bin/fish" -l -C "source '$ENV_DIR/config/fish/config.fish'"
"#,
        env_dir = env_dir.display()
    );

    let script_path = env_dir.join("activate.sh");
    fs::write(&script_path, script_content).context("Failed to write activate.sh")?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

/// Creates the `config.fish` file with initialization commands.
fn write_fish_config(env_dir: &Path) -> AppResult<()> {
    let fish_config_dir = env_dir.join("config").join("fish");
    fs::create_dir_all(&fish_config_dir).context("Failed to create fish config directory")?;

    let config_content = r#"# Starship prompt
starship init fish | source

# Atuin shell history
atuin init fish | source

# Zoxide directory jumper
zoxide init fish | source

# Welcome message
echo "Welcome to your isolated shell environment!"
echo "Type 'exit' to return to your regular shell."
"#;

    let config_path = fish_config_dir.join("config.fish");
    fs::write(config_path, config_content).context("Failed to write config.fish")?;
    Ok(())
}

/// Creates a default `starship.toml` configuration.
fn write_starship_config(env_dir: &Path) -> AppResult<()> {
    let config_path = env_dir.join("config").join("starship.toml");
    let config_content = r#"
# Inserts a blank line between shell prompts
add_newline = true

[character]
success_symbol = "[➜](bold green)"
error_symbol = "[➜](bold red)"
"#;
    fs::write(config_path, config_content).context("Failed to write starship.toml")?;
    Ok(())
}

/// Creates a default `atuin/config.toml` configuration.
fn write_atuin_config(env_dir: &Path) -> AppResult<()> {
    let atuin_config_dir = env_dir.join("config").join("atuin");
    fs::create_dir_all(&atuin_config_dir).context("Failed to create atuin config directory")?;

    let config_path = atuin_config_dir.join("config.toml");
    let config_content = r#"
# The database path for Atuin history.
# We'll keep it inside the environment's data directory.
db_path = "~/.local/share/atuin/history.db"

# How often to sync with the server.
sync_frequency = "5m"

# The address of the sync server.
sync_address = "https://api.atuin.sh"
"#;
    fs::write(config_path, config_content).context("Failed to write atuin/config.toml")?;
    Ok(())
}