use cmd_exists::cmd_exists;
use std::process::Command;

fn main() {
    if cmd_exists("fish").is_ok() {
        println!("Fish is already installed.");
        return;
    }

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