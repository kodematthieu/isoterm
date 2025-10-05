#!/bin/sh

# This script sets up the necessary environment variables and executes the fish shell.

set -e

# Determine the absolute path to the environment's root directory.
ENV_DIR=$(cd "$(dirname "$0")" && pwd)

# 1. PATH: Prepend the environment's bin directory to the system PATH.
export PATH="$ENV_DIR/bin:$PATH"

# 2. XDG_DATA_DIRS: Prepend the portable fish shell's data directory.
# This is the critical step that allows fish to find its standard library
# functions and completions, resolving the `__fish_data_dir` issue.
FISH_DATA_DIR="$ENV_DIR/fish_runtime/share"
if [ -n "$XDG_DATA_DIRS" ]; then
  export XDG_DATA_DIRS="$FISH_DATA_DIR:$XDG_DATA_DIRS"
else
  # Provide a sensible default if the variable is not already set.
  export XDG_DATA_DIRS="$FISH_DATA_DIR:/usr/local/share:/usr/share"
fi

# 3. Tool-specific Environment Variables: Point tools to their configs
# within the isolated environment.
export STARSHIP_CONFIG="$ENV_DIR/config/starship.toml"
export ATUIN_CONFIG_DIR="$ENV_DIR/config/atuin"
export HELIX_CONFIG="$ENV_DIR/config/helix/config.toml"

# 4. Execute fish: Replace the current shell process with fish.
# The `-C` flag executes a command, in this case sourcing our custom config.
exec "$ENV_DIR/bin/fish" -l -C "source '$ENV_DIR/config/fish/config.fish'"
