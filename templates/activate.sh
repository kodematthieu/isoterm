#!/bin/sh

# This script sets up the necessary environment variables and executes the fish shell.

set -e

# Determine the absolute path to the environment's root directory.
ENV_DIR=$(cd "$(dirname "$0")" && pwd)

# 1. PATH: Prepend the environment's bin directory to the system PATH.
export PATH="$ENV_DIR/bin:$PATH"

# 2. XDG_CONFIG_HOME & XDG_DATA_HOME: Redirect all configuration and data
#    lookups for XDG-compliant tools (Helix, Atuin, Starship) into the
#    isolated environment. This is the core of the isolation strategy.
export XDG_CONFIG_HOME="$ENV_DIR/config"
export XDG_DATA_HOME="$ENV_DIR/data"

# 3. XDG_DATA_DIRS: Prepend the portable fish shell's runtime data directory.
#    This is distinct from XDG_DATA_HOME and is critical for allowing fish
#    to find its standard library functions and completions.
FISH_RUNTIME_DATA_DIR="$ENV_DIR/fish_runtime/share"
if [ -n "$XDG_DATA_DIRS" ]; then
  export XDG_DATA_DIRS="$FISH_RUNTIME_DATA_DIR:$XDG_DATA_DIRS"
else
  # Provide a sensible default if the variable is not already set.
  export XDG_DATA_DIRS="$FISH_RUNTIME_DATA_DIR:/usr/local/share:/usr/share"
fi

# 4. Execute fish: Replace the current shell process with fish.
#    The `-C` flag executes a command, in this case sourcing our custom config,
#    which will be located at $XDG_CONFIG_HOME/fish/config.fish.
exec "$ENV_DIR/bin/fish" -l -C "source '$XDG_CONFIG_HOME/fish/config.fish'"
