# isoterm

`isoterm` is a tool for creating isolated, non-destructive, and portable shell environments.

Instead of modifying your system's global configuration or installing software with a package manager, this tool creates a self-contained directory that includes all the necessary binaries, configuration files, and an activation script. This approach ensures that your main system remains untouched and makes cleanup as simple as deleting a single folder.

The environment comes pre-configured with:
- **[Fish Shell](https://fishshell.com/):** A smart and user-friendly command-line shell.
- **[Starship](https://starship.rs/):** A minimal, blazing-fast, and infinitely customizable prompt.
- **[Zoxide](https://github.com/ajeetdsouza/zoxide):** A smarter `cd` command that learns your habits.
- **[Atuin](https://atuin.sh/):** A tool for magical shell history.

## How It Works

The tool prioritizes using what's already on your system. For each required command (`fish`, `starship`, etc.), it will:
1.  **Check the environment directory:** If the tool is already there, it does nothing.
2.  **Look on your system `PATH`:** If it finds an existing installation, it will create a **symlink** to it inside the environment's `bin/` directory.
3.  **Download a pre-compiled binary:** If the tool isn't found locally, it will download the latest official release from GitHub, extract it, and place the binary in the `bin/` directory.

This ensures the environment is lightweight and avoids redundant downloads.

## Usage

Create a new isolated environment by running the setup script. You can pipe it directly into `sh` from GitHub.

This script automatically detects your OS and architecture, downloads the correct pre-compiled binary, runs it to set up the environment, and then cleans up after itself. The `isoterm` binary is not permanently installed.

#### Create an environment in the default location (`~/.local_shell`):
```sh
curl -sSL "https://github.com/kodematthieu/isoterm/releases/latest/download/setup.sh" | sh
```

#### Create an environment in a specific directory:
You can pass arguments to the script, which are forwarded directly to the `isoterm` executable.
```sh
curl -sSL "https://github.com/kodematthieu/isoterm/releases/latest/download/setup.sh" | sh -s -- ./my-temp-env
```

The script will print its progress as it provisions each tool and generates the necessary configuration files.

### Activate the Environment
Once the setup is complete, you can activate the new environment by sourcing the `activate.sh` script.

```sh
# If you used a custom path
source ./my-temp-env/activate.sh

# If you used the default path
source ~/.local_shell/activate.sh
```
You will be dropped into a new `fish` shell session with all tools ready to use. To exit the environment, simply type `exit`.

## Building from Source

If you prefer to build the project from source (e.g., for development), you will need the Rust toolchain installed.

You can build the project using Cargo:
```sh
cargo build --release
```
The binary will be located at `target/release/isoterm`. You can then run it directly:
```sh
./target/release/isoterm ./my-dev-env
```

## Uninstallation Instructions

Because the environment is completely isolated, removal is trivial. Just delete the environment directory.

```sh
# If you used a custom path
rm -rf ./my-temp-env

# If you used the default path
rm -rf ~/.local_shell
```
There are no other files to clean up. Your system remains exactly as it was before.