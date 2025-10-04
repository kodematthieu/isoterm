#!/bin/sh

# This script downloads and executes isoterm.
# It is designed to be ephemeral, cleaning up after itself.
#
# Usage:
#   curl -sSL <URL>/setup.sh | sh
#   curl -sSL <URL>/setup.sh | sh -s -- ./my-custom-env

set -e # Exit immediately if a command exits with a non-zero status.

# --- Configuration ---
# NOTE: This VERSION placeholder will be replaced by the CI during the release process.
VERSION="<PLACEHOLDER_VERSION>"
GITHUB_USER="kodematthieu"
GITHUB_REPO="isoterm"
LATEST_API_URL="https://api.github.com/repos/${GITHUB_USER}/${GITHUB_REPO}/releases/latest"
TAG_API_URL_TEMPLATE="https://api.github.com/repos/${GITHUB_USER}/${GITHUB_REPO}/releases/tags/${VERSION}"

# --- Helper Functions ---
get_latest_tag() {
    # Fetches the tag name of the latest release.
    curl -s "$LATEST_API_URL" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

# --- Platform Detection ---
get_platform() {
  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)

  # Termux on Android detection (must come before generic Linux)
  if [ -n "$TERMUX_VERSION" ]; then
    case "$arch" in
      aarch64) target="aarch64-linux-android" ;;
      *) echo "Error: Unsupported architecture ($arch) for Termux." >&2; exit 1 ;;
    esac
    echo "$target"
    return
  fi

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
  api_url="$TAG_API_URL_TEMPLATE"
  release_source_msg="release '$VERSION'"

  # If the script is not versioned (i.e., placeholder wasn't replaced),
  # default to the latest stable release.
  if [ "$VERSION" = "<PLACEHOLDER_VERSION>" ]; then
    api_url="$LATEST_API_URL"
    release_source_msg="latest release"
  else
    # If the script IS versioned, check if a newer stable release exists.
    latest_tag=$(get_latest_tag)
    if [ -n "$latest_tag" ] && [ "$VERSION" != "$latest_tag" ]; then
      # Check if VERSION contains a hyphen to identify it as a pre-release
      case "$VERSION" in
        *-*)
          # It's a pre-release
          echo "Note: This setup script is for pre-release version '$VERSION', but the latest stable version is '$latest_tag'."
          printf "Would you like to download the latest stable version instead? [y/N] "
          ;;
        *)
          # It's an older stable version
          echo "Note: This setup script is for an older version '$VERSION', but the latest version is '$latest_tag'."
          printf "Would you like to download the latest version instead? [y/N] "
          ;;
      esac

      read -r response
      case "$response" in
        [yY][eE][sS]|[yY])
          api_url="$LATEST_API_URL"
          release_source_msg="latest release '$latest_tag'"
          ;;
        *)
          # Default is 'No', continue with the script's version
          ;;
      esac
    fi
  fi

  target=$(get_platform)
  echo "Platform detected: $target"
  echo "Fetching release information from $release_source_msg..."

  # Find download URL for the target asset from the selected API endpoint
  download_url=$(curl -s "$api_url" | grep "browser_download_url.*${target}\\.tar\\.gz" | cut -d '"' -f 4 | head -n 1)

  # Gracefully exit if no binary is found for the target platform.
  if [ -z "$download_url" ]; then
    echo "Note: A pre-compiled binary for your platform ($target) was not found in $release_source_msg."
    echo "This platform is not currently supported by this release."
    echo "You can check for available assets here: https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases"
    exit 0
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
