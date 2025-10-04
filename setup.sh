#!/bin/sh

# This script downloads and executes the latest version of isoterm.
# It is designed to be ephemeral, cleaning up after itself.
#
# Usage:
#   curl -sSL <URL>/setup.sh | sh
#   curl -sSL <URL>/setup.sh | sh -s -- ./my-custom-env

set -e # Exit immediately if a command exits with a non-zero status.

# --- Configuration ---
GITHUB_USER="kodematthieu"
GITHUB_REPO="isoterm"
API_URL="https://api.github.com/repos/${GITHUB_USER}/${GITHUB_REPO}/releases/latest"

# --- Platform Detection ---
get_platform() {
  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)

  case "$os" in
    linux)
      case "$arch" in
        x86_64) target="x86_64-unknown-linux-gnu" ;;
        aarch64) target="aarch64-unknown-linux-gnu" ;;
        *) echo "Error: Unsupported architecture ($arch) for Linux." >&2; exit 1 ;;
      esac
      ;;
    darwin)
      case "$arch" in
        x86_64) target="x86_64-apple-darwin" ;;
        arm64) target="aarch64-apple-darwin" ;; # uname -m on Apple Silicon is arm64
        *) echo "Error: Unsupported architecture ($arch) for macOS." >&2; exit 1 ;;
      esac
      ;;
    *)
      echo "Error: Unsupported operating system ($os)." >&2
      exit 1
      ;;
  esac
  echo "$target"
}

# --- Main Logic ---
main() {
  target=$(get_platform)
  echo "Platform detected: $target"

  # Find download URL for the target asset
  echo "Fetching latest release information..."
  download_url=$(curl -s "$API_URL" | grep "browser_download_url.*${target}\\.tar\\.gz" | cut -d '"' -f 4 | head -n 1)

  if [ -z "$download_url" ]; then
    echo "Error: Could not find a release asset for your platform ($target)." >&2
    echo "Please check the releases page: https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases" >&2
    exit 1
  fi

  # Create a temporary directory for the download and extraction
  tmp_dir=$(mktemp -d 2>/dev/null || mktemp -d -t 'isoterm-install')

  # Setup cleanup trap
  trap 'rm -rf "$tmp_dir"' EXIT

  archive_path="$tmp_dir/isoterm.tar.gz"

  echo "Downloading isoterm to a temporary location..."
  curl -L --progress-bar "$download_url" -o "$archive_path"

  echo "Extracting binary..."
  tar -xzf "$archive_path" -C "$tmp_dir"

  binary_path="$tmp_dir/isoterm"
  chmod +x "$binary_path"

  echo "Executing isoterm..."
  echo "--------------------------------------------------"

  # Execute the binary, passing all script arguments to it
  "$binary_path" "$@"

  echo "--------------------------------------------------"
  echo "isoterm execution finished. Cleaning up temporary files."
}

# --- Run ---
main "$@"
