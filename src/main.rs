use cmd_exists::cmd_exists;
use std::process::Command;

fn main() {
    if cmd_exists("fish").is_ok() {
        println!("Fish is already installed.");
    } else {
        println!("Fish is not installed. Installing...");

        if cfg!(target_os = "linux") {
            install_fish_linux();
        } else if cfg!(target_os = "macos") {
            install_fish_macos();
        } else if cfg!(target_os = "windows") {
            install_fish_windows();
        } else if cfg!(target_os = "android") {
            install_fish_android();
        } else {
            eprintln!("Unsupported operating system.");
        }
    }

    if cmd_exists("fish").is_ok() {
        install_additional_tools();
    } else {
        eprintln!("Fish installation failed or not found, skipping additional tools.");
    }
}

fn install_additional_tools() {
    println!("Checking for additional tools...");

    if !cmd_exists("atuin").is_ok() {
        println!("Atuin is not installed. Installing...");
        install_atuin();
    } else {
        println!("Atuin is already installed.");
    }

    if !cmd_exists("zoxide").is_ok() {
        println!("Zoxide is not installed. Installing...");
        install_zoxide();
    } else {
        println!("Zoxide is already installed.");
    }

    if !cmd_exists("starship").is_ok() {
        println!("Starship is not installed. Installing...");
        install_starship();
    } else {
        println!("Starship is already installed.");
    }
}

fn install_atuin() {
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        let mut command = Command::new("bash");
        command.args(&[
            "-c",
            "curl --proto '=https' --tlsv1.2 -LsSf https://github.com/atuinsh/atuin/releases/latest/download/atuin-installer.sh | sh -s -- -q",
        ]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Atuin installed successfully.");
        } else {
            eprintln!("Failed to install Atuin.");
        }
    } else if cfg!(target_os = "windows") {
        println!("Atuin installation on Windows requires `cargo`. Please install Rust and run `cargo install atuin` manually.");
    } else if cfg!(target_os = "android") {
        let mut command = Command::new("pkg");
        command.args(&["install", "-y", "atuin"]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Atuin installed successfully.");
        } else {
            eprintln!("Failed to install Atuin.");
        }
    } else {
        eprintln!("Unsupported OS for Atuin installation.");
    }
}

fn install_zoxide() {
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        let mut command = Command::new("bash");
        command.args(&[
            "-c",
            "curl -sS https://raw.githubusercontent.com/ajeetdsouza/zoxide/main/install.sh | bash -s -- --sudo ''",
        ]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Zoxide installed successfully.");
        } else {
            eprintln!("Failed to install Zoxide.");
        }
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("winget");
        command.args(&["install", "zoxide"]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Zoxide installed successfully.");
        } else {
            eprintln!("Failed to install Zoxide.");
        }
    } else if cfg!(target_os = "android") {
        let mut command = Command::new("pkg");
        command.args(&["install", "-y", "zoxide"]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Zoxide installed successfully.");
        } else {
            eprintln!("Failed to install Zoxide.");
        }
    } else {
        eprintln!("Unsupported OS for Zoxide installation.");
    }
}

fn install_starship() {
    if cfg!(any(target_os = "linux", target_os = "macos")) {
        let mut command = Command::new("bash");
        command.args(&[
            "-c",
            "curl -sS https://starship.rs/install.sh | sh -s -- -y --bin-dir $HOME/.local/bin",
        ]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Starship installed successfully.");
        } else {
            eprintln!("Failed to install Starship.");
        }
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("winget");
        command.args(&["install", "starship"]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Starship installed successfully.");
        } else {
            eprintln!("Failed to install Starship.");
        }
    } else if cfg!(target_os = "android") {
        let mut command = Command::new("pkg");
        command.args(&["install", "-y", "starship"]);
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Starship installed successfully.");
        } else {
            eprintln!("Failed to install Starship.");
        }
    } else {
        eprintln!("Unsupported OS for Starship installation.");
    }
}

fn install_fish_linux() {
    let mut cmd: Option<Command> = None;

    if cmd_exists("apt").is_ok() {
        let mut command = Command::new("sudo");
        command.args(&["apt", "install", "-y", "fish"]);
        cmd = Some(command);
    } else if cmd_exists("pacman").is_ok() {
        let mut command = Command::new("sudo");
        command.args(&["pacman", "-S", "--noconfirm", "fish"]);
        cmd = Some(command);
    } else if cmd_exists("dnf").is_ok() {
        let mut command = Command::new("sudo");
        command.args(&["dnf", "install", "-y", "fish"]);
        cmd = Some(command);
    }

    if let Some(mut command) = cmd {
        let status = command.status().expect("Failed to execute command");
        if status.success() {
            println!("Fish installed successfully.");
        } else {
            eprintln!("Failed to install fish.");
        }
    } else {
        eprintln!("Could not find a supported package manager (apt, pacman, dnf).");
    }
}

fn install_fish_macos() {
    if !cmd_exists("brew").is_ok() {
        eprintln!("Homebrew is not installed. Please install it first.");
        return;
    }

    let mut command = Command::new("brew");
    command.args(&["install", "fish"]);

    let status = command.status().expect("Failed to execute command");
    if status.success() {
        println!("Fish installed successfully.");
    } else {
        eprintln!("Failed to install fish.");
    }
}

fn install_fish_windows() {
    if !cmd_exists("winget").is_ok() {
        eprintln!("winget is not installed. Please install it first from the Microsoft Store.");
        return;
    }

    let mut command = Command::new("winget");
    command.args(&["install", "fish"]);

    let status = command.status().expect("Failed to execute command");
    if status.success() {
        println!("Fish installed successfully.");
    } else {
        eprintln!("Failed to install fish.");
    }
}

fn install_fish_android() {
    if !cmd_exists("pkg").is_ok() {
        eprintln!("Termux is not installed or `pkg` is not in your PATH.");
        return;
    }

    let mut command = Command::new("pkg");
    command.args(&["install", "-y", "fish"]);

    let status = command.status().expect("Failed to execute command");
    if status.success() {
        println!("Fish installed successfully.");
    } else {
        eprintln!("Failed to install fish.");
    }
}