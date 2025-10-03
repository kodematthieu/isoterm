use crate::error::AppResult;
use anyhow::{anyhow, Context};
use flate2::read::GzDecoder;
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{self, Cursor};
use std::path::Path;
use tar::Archive;
use xz2::read::XzDecoder;
use zip::ZipArchive;

#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// Represents a tool to be provisioned.
pub struct Tool {
    /// The name of the command, e.g., "fish", "starship".
    pub name: &'static str,
    /// The GitHub repository in "owner/repo" format.
    pub repo: &'static str,
    /// The name of the executable file inside the archive.
    pub binary_name: &'static str,
}

/// The main provisioning function for a single tool.
pub fn provision_tool(env_dir: &Path, tool: &Tool) -> AppResult<()> {
    let bin_dir = env_dir.join("bin");
    let tool_path_in_env = bin_dir.join(tool.binary_name);

    if tool_path_in_env.exists() {
        println!("✅ {} is already provisioned.", tool.name);
        return Ok(());
    }

    if let Ok(system_path) = which::which(tool.binary_name) {
        println!(
            "Found system-installed {} at: {}",
            tool.name,
            system_path.display()
        );
        if create_symlink(&system_path, &tool_path_in_env).is_ok() {
            println!("🔗 Symlinked existing {} to environment.", tool.name);
            return Ok(());
        }
        println!(
            "⚠️ Symlink failed for {}. Falling back to copying.",
            tool.name
        );
        if fs::copy(&system_path, &tool_path_in_env).is_ok() {
            println!("📋 Copied existing {} to environment.", tool.name);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = fs::metadata(&system_path) {
                    let perms = metadata.permissions();
                    if perms.mode() & 0o111 != 0 {
                        fs::set_permissions(&tool_path_in_env, perms)?;
                    }
                }
            }
            return Ok(());
        }
        println!(
            "❌ Failed to symlink or copy existing {}. Proceeding to download.",
            tool.name
        );
    }

    println!("⬇️ {} not found on system. Downloading...", tool.name);
    if tool.name == "fish" {
        download_and_install_fish(env_dir, tool)
    } else {
        download_and_install_binary(env_dir, tool)
    }
}

fn download_and_install_binary(env_dir: &Path, tool: &Tool) -> AppResult<()> {
    let (download_url, asset_name) = find_github_release_asset_url(tool)?;
    println!("Downloading from {}", download_url);

    let response = reqwest::blocking::get(download_url)
        .context("Failed to download asset")?
        .bytes()
        .context("Failed to read response bytes")?;

    let bin_dir = env_dir.join("bin");
    let tool_path = bin_dir.join(tool.binary_name);

    if asset_name.ends_with(".zip") {
        extract_zip(&response, &bin_dir, tool.binary_name)?;
    } else if asset_name.ends_with(".tar.gz") {
        extract_tar_gz(&response, &bin_dir, tool.binary_name)?;
    } else {
        return Err(anyhow!("Unsupported archive format for {}", asset_name));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tool_path, fs::Permissions::from_mode(0o755))?;
    }

    println!(
        "✅ Successfully installed {} to {}",
        tool.name,
        tool_path.display()
    );
    Ok(())
}

fn download_and_install_fish(env_dir: &Path, tool: &Tool) -> AppResult<()> {
    let (download_url, _asset_name) = find_github_release_asset_url(tool)?;
    println!("Downloading fish from {}", download_url);

    let response = reqwest::blocking::get(&download_url)
        .context("Failed to download fish asset")?
        .bytes()
        .context("Failed to read response bytes")?;

    let fish_runtime_dir = env_dir.join("fish_runtime");
    fs::create_dir_all(&fish_runtime_dir).context("Failed to create fish_runtime directory")?;

    let tar = XzDecoder::new(&response[..]);
    let mut archive = Archive::new(tar);

    // Extract entire archive into fish_runtime_dir, stripping the top-level component
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        if let Some(path_stripped) = path.strip_prefix(path.components().next().unwrap()).ok() {
            if !path_stripped.as_os_str().is_empty() {
                entry.unpack(fish_runtime_dir.join(path_stripped))?;
            }
        }
    }

    // Create symlink
    let fish_binary_in_runtime = fish_runtime_dir.join("bin").join("fish");
    let fish_binary_in_env = env_dir.join("bin").join("fish");

    if !fish_binary_in_runtime.exists() {
        return Err(anyhow!(
            "Could not find fish binary in extracted archive at {}",
            fish_binary_in_runtime.display()
        ));
    }

    create_symlink(&fish_binary_in_runtime, &fish_binary_in_env)?;

    println!("✅ Successfully installed fish.");
    Ok(())
}

fn find_github_release_asset_url(tool: &Tool) -> AppResult<(String, String)> {
    let repo_url = format!("https://api.github.com/repos/{}/releases/latest", tool.repo);
    let client = reqwest::blocking::Client::builder()
        .user_agent("auto-term-setup")
        .build()?;

    let response: Value = client
        .get(&repo_url)
        .send()
        .context("Failed to query GitHub API")?
        .json()
        .context("Failed to parse JSON response from GitHub API")?;

    let assets = response["assets"]
        .as_array()
        .ok_or_else(|| anyhow!("No assets found in release for {}", tool.repo))?;

    let arch = env::consts::ARCH;
    let os_str = match env::consts::OS {
        "linux" => {
            if tool.name == "zoxide" {
                "unknown-linux-musl"
            } else {
                "unknown-linux-gnu"
            }
        }
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        _ => return Err(anyhow!("Unsupported OS: {}", env::consts::OS)),
    };
    let ext = if env::consts::OS == "windows" {
        "zip"
    } else if tool.name == "fish" {
        "tar.xz"
    } else {
        "tar.gz"
    };

    // The asset name contains fragments for the tool name, architecture, OS, and extension.
    // We build a list of fragments to search for in the asset names.
    // E.g., for starship on linux: ["starship", "x86_64", "unknown-linux-gnu", "tar.gz"]
    let fragments_to_use = vec![tool.name, arch, os_str, ext];

    for asset in assets {
        let name = asset["name"].as_str().unwrap_or("");
        let lower_name = name.to_lowercase();

        if fragments_to_use
            .iter()
            .all(|frag| lower_name.contains(&frag.to_lowercase()))
        {
            let url = asset["browser_download_url"].as_str().unwrap_or("").to_string();
            if !url.is_empty() {
                return Ok((url, name.to_string()));
            }
        }
    }

    Err(anyhow!(
        "Could not find a matching release asset for {} with fragments: {:?}",
        tool.name,
        fragments_to_use
    ))
}

fn extract_tar_gz(bytes: &[u8], target_dir: &Path, binary_name: &str) -> AppResult<()> {
    let tar = GzDecoder::new(bytes);
    let mut archive = Archive::new(tar);

    for entry in archive.entries()? {
        let mut entry = entry?;
        if let Some(path) = entry.path().ok() {
            if path.file_name().map_or(false, |n| n == binary_name) {
                let target_path = target_dir.join(binary_name);
                entry.unpack(&target_path)?;
                return Ok(());
            }
        }
    }

    Err(anyhow!(
        "Could not find '{}' in the downloaded .tar.gz archive.",
        binary_name
    ))
}

fn extract_zip(bytes: &[u8], target_dir: &Path, binary_name: &str) -> AppResult<()> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if let Some(outpath) = file.enclosed_name() {
            if outpath.file_name().map_or(false, |n| n == binary_name) {
                let target_path = target_dir.join(binary_name);
                let mut outfile = File::create(&target_path)?;
                io::copy(&mut file, &mut outfile)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&target_path, fs::Permissions::from_mode(0o755))?;
                }

                return Ok(());
            }
        }
    }

    Err(anyhow!(
        "Could not find '{}' in the downloaded .zip archive.",
        binary_name
    ))
}

#[cfg(unix)]
fn create_symlink(original: &Path, link: &Path) -> AppResult<()> {
    symlink(original, link).context("Failed to create symlink")
}

#[cfg(windows)]
fn create_symlink(original: &Path, link: &Path) -> AppResult<()> {
    if original.is_dir() {
        symlink_dir(original, link).context("Failed to create directory symlink")
    } else {
        symlink_file(original, link).context("Failed to create file symlink")
    }
}