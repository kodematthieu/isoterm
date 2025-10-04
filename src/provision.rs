use crate::error::AppResult;
use anyhow::{anyhow, Context};
use console::style;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use pathdiff;
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::Path;
use tar::Archive;
use tempfile::NamedTempFile;
use xz2::read::XzDecoder;
use zip::ZipArchive;

#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

/// Represents a tool to be provisioned.
#[derive(Debug)]
pub struct Tool {
    pub name: &'static str,
    pub repo: &'static str,
    pub binary_name: &'static str,
    pub path_in_archive: Option<&'static str>,
}

/// Main provisioning function for a single tool.
#[tracing::instrument(skip(env_dir, tool, pb, spinner_style))]
pub async fn provision_tool(
    env_dir: &Path,
    tool: &Tool,
    pb: &ProgressBar,
    spinner_style: &ProgressStyle,
) -> AppResult<()> {
    pb.set_message(format!("Provisioning {}...", style(tool.name).bold()));

    let bin_dir = env_dir.join("bin");
    let tool_path_in_env = bin_dir.join(tool.binary_name);

    if tool_path_in_env.exists() {
        pb.finish_with_message(format!(
            "{} {} is already provisioned",
            style("✓").green(),
            style(tool.name).bold()
        ));
        return Ok(());
    }

    if let Ok(system_path) = which::which(tool.binary_name) {
        pb.set_message(format!(
            "Found {}, creating symlink...",
            style(tool.name).bold()
        ));
        if create_symlink(&system_path, &tool_path_in_env).is_ok() {
            pb.finish_with_message(format!(
                "{} Symlinked {} from {}",
                style("✓").green(),
                style(tool.name).bold(),
                style(system_path.display()).cyan()
            ));
            return Ok(());
        }
    }

    let download_result = if tool.path_in_archive.is_some() {
        download_and_install_archive(env_dir, tool, pb, spinner_style).await
    } else {
        download_and_install_binary(env_dir, tool, pb, spinner_style).await
    };

    if let Err(e) = download_result {
        pb.abandon_with_message(format!(
            "{} Failed to provision {}: {}",
            style("✗").red(),
            style(tool.name).bold(),
            e
        ));
        return Err(e);
    }
    Ok(())
}

/// Downloads a file to a temporary file on disk, showing progress.
async fn download_to_temp_file(
    url: &str,
    asset_name: &str,
    pb: &ProgressBar,
) -> AppResult<NamedTempFile> {
    let response = reqwest::get(url).await?.error_for_status()?;
    let total_size = response.content_length().unwrap_or(0);

    let download_style = ProgressStyle::with_template(
        "{spinner:.green} {msg}\n{wide_bar:.cyan/blue} {bytes}/{total_bytes} ({eta})",
    )?
    .progress_chars("#>-");

    pb.set_style(download_style);
    pb.set_length(total_size);
    pb.set_message(format!("Downloading {}", style(asset_name).cyan()));

    let mut temp_file = NamedTempFile::new()?;
    let mut stream = response.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.context("Failed to read download chunk")?;
        temp_file.write_all(&chunk)?;
        pb.inc(chunk.len() as u64);
    }

    Ok(temp_file)
}

#[tracing::instrument(skip(env_dir, tool, pb, spinner_style))]
async fn download_and_install_binary(
    env_dir: &Path,
    tool: &Tool,
    pb: &ProgressBar,
    spinner_style: &ProgressStyle,
) -> AppResult<()> {
    let (download_url, asset_name) = find_github_release_asset_url(
        tool,
        "https://api.github.com",
        env::consts::OS,
        env::consts::ARCH,
    )
    .await?;
    let temp_file = download_to_temp_file(&download_url, &asset_name, pb).await?;

    pb.set_style(spinner_style.clone());
    pb.set_message(format!(
        "Extracting {}...",
        style(tool.binary_name).bold()
    ));

    let bin_dir = env_dir.join("bin");
    let tool_path = bin_dir.join(tool.binary_name);
    let mut file = temp_file.reopen()?;

    if asset_name.ends_with(".zip") {
        extract_zip(&mut file, &bin_dir, tool.binary_name)?;
    } else if asset_name.ends_with(".tar.gz") {
        extract_tar_gz(&mut file, &bin_dir, tool.binary_name)?;
    } else {
        return Err(anyhow!("Unsupported archive format for {}", asset_name));
    }

    #[cfg(unix)]
    {
        fs::set_permissions(&tool_path, fs::Permissions::from_mode(0o755))?;
    }

    pb.finish_with_message(format!(
        "{} Installed {} to {}",
        style("✓").green(),
        style(tool.name).bold(),
        style(tool_path.display()).cyan()
    ));
    Ok(())
}

#[tracing::instrument(skip(env_dir, tool, pb, spinner_style))]
async fn download_and_install_archive(
    env_dir: &Path,
    tool: &Tool,
    pb: &ProgressBar,
    spinner_style: &ProgressStyle,
) -> AppResult<()> {
    let (download_url, asset_name) = find_github_release_asset_url(
        tool,
        "https://api.github.com",
        env::consts::OS,
        env::consts::ARCH,
    )
    .await?;
    let temp_file = download_to_temp_file(&download_url, &asset_name, pb).await?;
    let file = temp_file.reopen()?;

    pb.set_style(spinner_style.clone());
    pb.set_message(format!("Extracting archive for {}...", style(tool.name).bold()));

    let tool_dir = if tool.name == "fish" {
        env_dir.join("fish_runtime")
    } else {
        env_dir.join(tool.name)
    };
    fs::create_dir_all(&tool_dir)?;

    if asset_name.ends_with(".tar.gz") {
        let tar = GzDecoder::new(file);
        let mut archive = Archive::new(tar);
        extract_archive(&mut archive, &tool_dir)?;
    } else if asset_name.ends_with(".tar.xz") {
        let tar = XzDecoder::new(file);
        let mut archive = Archive::new(tar);
        extract_archive(&mut archive, &tool_dir)?;
    } else if asset_name.ends_with(".zip") {
        let mut zip = ZipArchive::new(file)?;
        extract_zip_archive(&mut zip, &tool_dir)?;
    } else {
        return Err(anyhow!("Unsupported archive format: {}", asset_name));
    }

    let binary_path_in_archive = tool_dir.join(tool.path_in_archive.unwrap());
    let binary_path_in_env = env_dir.join("bin").join(tool.binary_name);

    create_symlink(&binary_path_in_archive, &binary_path_in_env)?;

    pb.finish_with_message(format!(
        "{} Installed {} successfully",
        style("✓").green(),
        style(tool.name).bold()
    ));
    Ok(())
}

#[tracing::instrument(skip(base_url, os, arch))]
async fn find_github_release_asset_url(
    tool: &Tool,
    base_url: &str,
    os: &str,
    arch: &str,
) -> AppResult<(String, String)> {
    let repo_url = format!("{}/repos/{}/releases/latest", base_url, tool.repo);
    let client = reqwest::Client::builder().user_agent("isoterm").build()?;

    let response: Value = client
        .get(&repo_url)
        .send()
        .await
        .context("Failed to query GitHub API")?
        .json()
        .await
        .context("Failed to parse JSON response from GitHub API")?;

    let assets = response["assets"]
        .as_array()
        .ok_or_else(|| anyhow!("No assets found in release for {}", tool.repo))?;

    let os_targets: Vec<&str> = match os {
        "linux" => match tool.name {
            "fish" | "helix" => vec!["linux"],
            _ => vec!["unknown-linux-gnu", "unknown-linux-musl"],
        },
        "macos" => vec!["apple-darwin"],
        "windows" => vec!["pc-windows-msvc"],
        _ => return Err(anyhow!("Unsupported OS: {}", os)),
    };

    let ext = if os == "windows" {
        "zip"
    } else {
        match tool.name {
            "helix" if os == "linux" => "tar.xz",
            "helix" if os == "macos" => "zip",
            "fish" => "tar.xz",
            _ => "tar.gz",
        }
    };

    for os_target in &os_targets {
        let fragments_to_use = vec![tool.name, arch, *os_target, ext];

        tracing::debug!(fragments = ?fragments_to_use, "Searching for asset");

        for asset in assets {
            let name = asset["name"].as_str().unwrap_or("");
            let lower_name = name.to_lowercase();

            if fragments_to_use
                .iter()
                .all(|frag| lower_name.contains(&frag.to_lowercase()))
            {
                if let Some(url) = asset["browser_download_url"].as_str() {
                    tracing::info!(asset = name, "Found matching release asset");
                    return Ok((url.to_string(), name.to_string()));
                }
            }
        }
    }

    Err(anyhow!(
        "Could not find a matching release asset for {} on {} {}",
        tool.name,
        env::consts::OS,
        arch
    ))
}

#[tracing::instrument(skip(reader))]
fn extract_tar_gz<R: Read>(reader: R, target_dir: &Path, binary_name: &str) -> AppResult<()> {
    let tar = GzDecoder::new(reader);
    let mut archive = Archive::new(tar);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry
            .path()?
            .file_name()
            .map_or(false, |n| n == binary_name)
        {
            entry.unpack(target_dir.join(binary_name))?;
            return Ok(());
        }
    }
    Err(anyhow!(
        "Could not find '{}' in the downloaded .tar.gz archive.",
        binary_name
    ))
}

fn extract_archive<R: io::Read>(archive: &mut Archive<R>, target_dir: &Path) -> AppResult<()> {
    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?.to_path_buf();

        // If the path has more than one component, it's nested in a top-level directory.
        // In that case, we strip the top-level directory. Otherwise, we use the path as is.
        let stripped_path = if path.components().count() > 1 {
            path.strip_prefix(path.components().next().unwrap())
                .unwrap_or(&path)
        } else {
            &path
        };
        let outpath = target_dir.join(stripped_path);

        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            entry.unpack(&outpath)?;
        }
    }
    Ok(())
}

fn extract_zip_archive<R: io::Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    target_dir: &Path,
) -> AppResult<()> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if let Some(enclosed_name) = file.enclosed_name() {
            let stripped_path = enclosed_name
                .strip_prefix(enclosed_name.components().next().unwrap())
                .unwrap_or(&enclosed_name);
            let outpath = target_dir.join(stripped_path);
            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = fs::File::create(&outpath)?;
                io::copy(&mut file, &mut outfile)?;
            }
            #[cfg(unix)]
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}

#[tracing::instrument(skip(reader))]
fn extract_zip<R: Read + Seek>(
    reader: R,
    target_dir: &Path,
    binary_name: &str,
) -> AppResult<()> {
    let mut archive = ZipArchive::new(reader)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file
            .enclosed_name()
            .map_or(false, |p| p.file_name().map_or(false, |n| n == binary_name))
        {
            let target_path = target_dir.join(binary_name);
            let mut outfile = File::create(&target_path)?;
            io::copy(&mut file, &mut outfile)?;
            #[cfg(unix)]
            {
                fs::set_permissions(&target_path, fs::Permissions::from_mode(0o755))?;
            }
            return Ok(());
        }
    }
    Err(anyhow!(
        "Could not find '{}' in the downloaded .zip archive.",
        binary_name
    ))
}

#[cfg(unix)]
#[tracing::instrument]
fn create_symlink(original: &Path, link: &Path) -> AppResult<()> {
    let link_parent = link.parent().unwrap_or_else(|| Path::new(""));
    let relative_path = pathdiff::diff_paths(original, link_parent)
        .ok_or_else(|| anyhow!("Failed to calculate relative path for symlink"))?;
    symlink(relative_path, link).context("Failed to create symlink")
}

#[cfg(windows)]
#[tracing::instrument]
fn create_symlink(original: &Path, link: &Path) -> AppResult<()> {
    let link_parent = link.parent().unwrap_or_else(|| Path::new(""));
    let relative_path = pathdiff::diff_paths(original, link_parent)
        .ok_or_else(|| anyhow!("Failed to calculate relative path for symlink"))?;

    if original.is_dir() {
        symlink_dir(relative_path, link).context("Failed to create directory symlink")
    } else {
        symlink_file(relative_path, link).context("Failed to create file symlink")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    #[cfg(unix)]
    fn test_create_relative_symlink_unix() -> AppResult<()> {
        let dir = tempdir()?;
        let tool_dir = dir.path().join("my_tool");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&tool_dir)?;
        fs::create_dir_all(&bin_dir)?;

        let original_path = tool_dir.join("my_binary");
        let link_path = bin_dir.join("my_binary_link");

        // Create a dummy file to link to
        fs::write(&original_path, "binary content")?;

        create_symlink(&original_path, &link_path)?;

        // Verify that the link exists and is a symlink
        assert!(link_path.exists());
        assert!(fs::symlink_metadata(&link_path)?.file_type().is_symlink());

        // Verify that the link is relative
        let link_target = fs::read_link(&link_path)?;
        assert_eq!(link_target.to_str(), Some("../my_tool/my_binary"));

        // Verify that the resolved path is correct
        let resolved_path = fs::canonicalize(&link_path)?;
        assert_eq!(resolved_path, fs::canonicalize(&original_path)?);

        Ok(())
    }

    #[test]
    #[cfg(windows)]
    fn test_create_relative_symlink_windows() -> AppResult<()> {
        let dir = tempdir()?;
        let tool_dir = dir.path().join("my_tool");
        let bin_dir = dir.path().join("bin");
        fs::create_dir_all(&tool_dir)?;
        fs::create_dir_all(&bin_dir)?;

        let original_path = tool_dir.join("my_binary");
        let link_path = bin_dir.join("my_binary_link");

        // Create a dummy file to link to
        fs::write(&original_path, "binary content")?;

        create_symlink(&original_path, &link_path)?;

        // Verify that the link exists and is a symlink
        assert!(link_path.exists());
        assert!(fs::symlink_metadata(&link_path)?.file_type().is_symlink());

        // Verify that the resolved path is correct
        let resolved_path = fs::canonicalize(&link_path)?;
        assert_eq!(resolved_path, fs::canonicalize(&original_path)?);

        Ok(())
    }

    #[tokio::test]
    async fn test_find_github_release_asset_url_ripgrep_logic() -> AppResult<()> {
        // Start a mock server.
        let mock_server = wiremock::MockServer::start().await;
        // Mock the GitHub API response.
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path(
                "/repos/BurntSushi/ripgrep/releases/latest",
            ))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "assets": [
                    {
                        "name": "decoy-asset-14.1.0-x86_64-unknown-linux-musl.tar.gz",
                        "browser_download_url": "https://example.com/decoy.tar.gz"
                    },
                    {
                        "name": "ripgrep-14.1.0-x86_64-unknown-linux-musl.tar.gz",
                        "browser_download_url": "https://example.com/ripgrep.tar.gz"
                    }
                ]
            })))
            .mount(&mock_server)
            .await;

        let tool = Tool {
            name: "ripgrep",
            repo: "BurntSushi/ripgrep",
            binary_name: "rg",
            path_in_archive: None,
        };

        let (url, name) =
            find_github_release_asset_url(&tool, &mock_server.uri(), "linux", "x86_64").await?;

        // This assertion will fail with the buggy logic but pass with the fix.
        assert_eq!(
            name, "ripgrep-14.1.0-x86_64-unknown-linux-musl.tar.gz",
            "The correct asset should be selected after the fix"
        );
        assert_eq!(
            url, "https://example.com/ripgrep.tar.gz",
            "The correct asset URL should be selected after the fix"
        );

        Ok(())
    }
}