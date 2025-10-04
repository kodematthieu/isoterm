#!/bin/sh

ENV_DIR=$(cd "$(dirname "$0")" && pwd)
PATH="$ENV_DIR/bin:$PATH" \
  STARSHIP_CONFIG="$ENV_DIR/config/starship.toml" \
  ATUIN_CONFIG_DIR="$ENV_DIR/config/atuin" \
  HELIX_CONFIG="$ENV_DIR/config/helix/config.toml" \
  FISH_HOME="$ENV_DIR/fish_runtime" \
  "$ENV_DIR/bin/fish" -l -C "source '$ENV_DIR/config/fish/config.fish'"