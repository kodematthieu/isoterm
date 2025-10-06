use crate::error::AppResult;
use anyhow::{Context, anyhow};
use console::style;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use pathdiff;
use regex::Regex;
use serde_json::Value;
use std::env;
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use tar::Archive;
use tempfile::NamedTempFile;
use tokio_retry::Retry;
use tokio_retry::strategy::{ExponentialBackoff, jitter};
use xz2::read::XzDecoder;
use zip::ZipArchive;

#[cfg(unix)]
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(windows)]
use std::os::windows::fs::{symlink_dir, symlink_file};

// --- Module Declarations ---
pub mod atuin;
pub mod fish;
pub mod helix;
pub mod ripgrep;
pub mod starship;
pub mod zoxide;

// --- Tool Trait ---
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn repo(&self) -> &'static str;
    fn binary_name(&self) -> &'static str;

    /// The path of the binary within the downloaded archive, if it's not at the root.
    fn path_in_archive(&self) -> Option<&'static str> {
        None
    }

    /// The main provisioning logic for downloading and installing from a remote source.
    /// The default implementation downloads a single binary from a GitHub release.
    /// More complex tools (like fish, helix) will override this.
    #[tracing::instrument(skip(self, context, pb, spinner_style), fields(tool = self.name()))]
    async fn provision_from_source(
        &self,
        context: &ProvisionContext,
        pb: &ProgressBar,
        spinner_style: &ProgressStyle,
    ) -> AppResult<()> {
        download_and_install_binary(
            &context.env_dir,
            self.name(),
            self.repo(),
            self.binary_name(),
            pb,
            spinner_style,
            &context.client,
        )
        .await
    }

    /// A hook that runs after a symlink is created to a system-provided tool.
    /// This is used by Helix to provision the runtime files even when the main binary is from the system.
    #[tracing::instrument(skip(self, _context, _pb, _system_path), fields(tool = self.name()))]
    async fn post_symlink_hook(
        &self,
        _context: &ProvisionContext,
        _pb: &ProgressBar,
        _system_path: &Path,
    ) -> AppResult<()> {
        // Default is to do nothing.
        tracing::debug!("No post-symlink hook for this tool.");
        Ok(())
    }
}

/// A context struct to pass shared, read-only data to provisioning tasks.
#[derive(Clone)]
pub struct ProvisionContext {
    pub env_dir: PathBuf,
    pub client: reqwest::Client,
}

// --- Generic Provisioning Orchestrator ---

#[tracing::instrument(skip(tool, context, pb, spinner_style), fields(tool = tool.name()))]
pub async fn provision_tool<T: Tool>(
    tool: T,
    context: &ProvisionContext,
    pb: &ProgressBar,
    spinner_style: &ProgressStyle,
) -> AppResult<()> {
    pb.set_message(format!("Provisioning {}...", style(tool.name()).bold()));

    let bin_dir = context.env_dir.join("bin");
    let tool_path_in_env = bin_dir.join(tool.binary_name());

    // 1. Check if the binary is already provisioned in our environment.
    if tool_path_in_env.exists() {
        tracing::debug!(path = %tool_path_in_env.display(), "Tool already exists, skipping provisioning.");
        pb.finish_with_message(format!(
            "{} {} is already provisioned",
            style("✓").green(),
            style(tool.name()).bold()
        ));
        return Ok(());
    }

    // 2. Check if the tool is available on the system PATH.
    if let Ok(system_path) = which::which(tool.binary_name()) {
        tracing::debug!(path = %system_path.display(), "Found tool on system");
        pb.set_message(format!(
            "Found {}, creating symlink...",
            style(tool.name()).bold()
        ));
        create_symlink(&system_path, &tool_path_in_env)?;

        // Run the post-symlink hook (for Helix runtime, etc.)
        tool.post_symlink_hook(context, pb, &system_path).await?;

        pb.finish_with_message(format!(
            "{} Symlinked {} from {}",
            style("✓").green(),
            style(tool.name()).bold(),
            style(system_path.display()).cyan()
        ));
        return Ok(());
    }

    // 3. If not found locally or on PATH, provision from source.
    tool.provision_from_source(context, pb, spinner_style)
        .await?;

    pb.finish_with_message(format!(
        "{} {} provisioned successfully",
        style("✓").green(),
        style(tool.name()).bold()
    ));

    Ok(())
}

// --- Helper Functions ---

/// Attempts to get the system's glibc version.
/// Returns a tuple of (major, minor) version numbers on success.
#[cfg(target_os = "linux")]
fn get_glibc_version() -> Option<(u32, u32)> {
    let mut child = Command::new("ldd")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null()) // Redirect stderr to prevent it from blocking
        .spawn()
        .ok()?;

    // Take ownership of the stdout pipe.
    let mut stdout_pipe = child.stdout.take()?;

    // Spawn a thread to read stdout asynchronously. This prevents a deadlock
    // if the child process generates a lot of output and fills the buffer.
    let stdout_thread = thread::spawn(move || {
        let mut stdout = String::new();
        stdout_pipe.read_to_string(&mut stdout).ok()?;
        Some(stdout)
    });

    // Retrieve the output from the reader thread *before* waiting on the child.
    // This is crucial to prevent a deadlock where the child fills the stdout
    // buffer and waits for the parent to read, while the parent is waiting for
    // the child to exit.
    let stdout = stdout_thread.join().ok()??;

    // Now, wait for the process to complete.
    let status = child.wait().ok()?;
    if !status.success() {
        return None;
    }
    let first_line = stdout.lines().next()?;

    // Regex to find version numbers like "2.35"
    let re = Regex::new(r"(\d+)\.(\d+)").ok()?;

    // Find the last match on the first line, as that's typically the version number.
    let caps = re.captures_iter(first_line).last()?;

    let major = caps.get(1)?.as_str().parse().ok()?;
    let minor = caps.get(2)?.as_str().parse().ok()?;

    tracing::debug!("Detected glibc version {}.{}", major, minor);
    Some((major, minor))
}

/// Downloads a file to a temporary file on disk, showing progress.
async fn download_to_temp_file(
    url: &str,
    asset_name: &str,
    pb: &ProgressBar,
    client: &reqwest::Client,
) -> AppResult<NamedTempFile> {
    let retry_strategy = ExponentialBackoff::from_millis(500).map(jitter).take(3);

    let result = Retry::spawn(retry_strategy, || async {
        // Reset progress bar on each attempt
        pb.set_position(0);

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let total_size = response.content_length().unwrap_or(0);

        let download_style = ProgressStyle::with_template(
            "{spinner:.green} {msg}\n{wide_bar:.cyan/blue} {bytes}/{total_bytes} ({eta})",
        )
        .map_err(|e| e.to_string())?
        .progress_chars("#>-");

        pb.set_style(download_style);
        pb.set_length(total_size);
        pb.set_message(format!("Downloading {}", style(asset_name).cyan()));

        let mut temp_file = NamedTempFile::new().map_err(|e| e.to_string())?;
        let mut stream = response.bytes_stream();

        while let Some(item) = stream.next().await {
            let chunk = item.map_err(|e| format!("Failed to read download chunk: {}", e))?;
            temp_file.write_all(&chunk).map_err(|e| e.to_string())?;
            pb.inc(chunk.len() as u64);
        }

        Ok(temp_file)
    })
    .await;

    result.map_err(|e: String| anyhow!(e))
}

#[tracing::instrument(skip(env_dir, pb, spinner_style, client))]
pub async fn download_and_install_binary(
    env_dir: &Path,
    name: &str,
    repo: &str,
    binary_name: &str,
    pb: &ProgressBar,
    spinner_style: &ProgressStyle,
    client: &reqwest::Client,
) -> AppResult<()> {
    let (download_url, asset_name) = find_github_release_asset_url(
        name,
        repo,
        "https://api.github.com",
        env::consts::OS,
        env::consts::ARCH,
        client,
    )
    .await?;
    let temp_file = download_to_temp_file(&download_url, &asset_name, pb, client).await?;

    pb.set_style(spinner_style.clone());
    pb.set_message(format!("Extracting {}...", style(binary_name).bold()));

    let bin_dir = env_dir.join("bin");
    let tool_path = bin_dir.join(binary_name);
    let mut file = temp_file.reopen()?;

    if asset_name.ends_with(".zip") {
        extract_zip(&mut file, &bin_dir, binary_name)?;
    } else if asset_name.ends_with(".tar.gz") {
        extract_tar_gz(&mut file, &bin_dir, binary_name)?;
    } else if asset_name.ends_with(".tar.xz") {
        extract_tar_xz(&mut file, &bin_dir, binary_name)?;
    } else {
        return Err(anyhow!("Unsupported archive format for {}", asset_name));
    }

    #[cfg(unix)]
    {
        fs::set_permissions(&tool_path, fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}

#[tracing::instrument(skip(env_dir, pb, spinner_style, client))]
pub async fn download_and_install_archive(
    env_dir: &Path,
    name: &str,
    repo: &str,
    binary_name: &str,
    path_in_archive: &str,
    pb: &ProgressBar,
    spinner_style: &ProgressStyle,
    client: &reqwest::Client,
) -> AppResult<()> {
    let (download_url, asset_name) = find_github_release_asset_url(
        name,
        repo,
        "https://api.github.com",
        env::consts::OS,
        env::consts::ARCH,
        client,
    )
    .await?;
    let temp_file = download_to_temp_file(&download_url, &asset_name, pb, client).await?;
    let file = temp_file.reopen()?;

    pb.set_style(spinner_style.clone());
    pb.set_message(format!("Extracting archive for {}...", style(name).bold()));

    let tool_dir = env_dir.join(name);
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

    let binary_path_in_archive = tool_dir.join(path_in_archive);
    let binary_path_in_env = env_dir.join("bin").join(binary_name);

    create_symlink(&binary_path_in_archive, &binary_path_in_env)?;

    pb.finish_with_message(format!(
        "{} Installed {} successfully",
        style("✓").green(),
        style(name).bold()
    ));
    Ok(())
}

#[tracing::instrument(skip(pb, client), fields(name = name, dest_dir = %dest_dir.display()))]
pub async fn provision_source_share(
    dest_dir: &Path,
    name: &str,
    repo: &str,
    pb: &ProgressBar,
    client: &reqwest::Client,
) -> AppResult<()> {
    pb.set_message(format!(
        "Downloading {} source for 'share' dir...",
        style(name).bold()
    ));

    // 1. Get the source tarball URL
    let (source_url, asset_name) =
        find_github_source_tarball_url(repo, "https://api.github.com", client).await?;

    // 2. Download to a temp file
    let temp_file = download_to_temp_file(&source_url, &asset_name, pb, client).await?;
    let mut file = temp_file.reopen()?;

    // 3. Selectively extract the 'share' directory
    pb.set_message(format!("Extracting 'share' for {}...", style(name).bold()));
    extract_share_from_tar_gz(&mut file, dest_dir)?;

    Ok(())
}

#[tracing::instrument(skip(client), fields(repo = repo))]
async fn find_github_source_tarball_url(
    repo: &str,
    base_url: &str,
    client: &reqwest::Client,
) -> AppResult<(String, String)> {
    let retry_strategy = ExponentialBackoff::from_millis(500).map(jitter).take(3);

    let result = Retry::spawn(retry_strategy, || async {
        let repo_url = format!("{}/repos/{}/releases/latest", base_url, repo);
        tracing::debug!(url = %repo_url, "Fetching latest release from GitHub API");

        let response: Value = client
            .get(&repo_url)
            .send()
            .await
            .map_err(|e| format!("Failed to query GitHub API: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse JSON response from GitHub API: {}", e))?;

        let tarball_url = response["tarball_url"].as_str().ok_or_else(|| {
            format!(
                "No 'tarball_url' found in release for {}. The API response may have changed.",
                repo
            )
        })?;

        let tag_name = response["tag_name"]
            .as_str()
            .unwrap_or("source")
            .to_string();

        tracing::info!(url = tarball_url, "Found source tarball URL");
        Ok((tarball_url.to_string(), tag_name))
    })
    .await;

    result.map_err(|e: String| anyhow!(e))
}

#[tracing::instrument(skip(client), fields(repo = repo, os = os, arch = arch))]
async fn find_github_release_asset_url(
    name: &str,
    repo: &str,
    base_url: &str,
    os: &str,
    arch: &str,
    client: &reqwest::Client,
) -> AppResult<(String, String)> {
    let retry_strategy = ExponentialBackoff::from_millis(500).map(jitter).take(3);

    let result = Retry::spawn(retry_strategy, || async {
        let repo_url = format!("{}/repos/{}/releases/latest", base_url, repo);
        tracing::debug!(url = %repo_url, "Fetching latest release from GitHub API");

        let response: Value = client
            .get(&repo_url)
            .send()
            .await
            .map_err(|e| format!("Failed to query GitHub API: {}", e))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse JSON response from GitHub API: {}", e))?;

        let assets = response["assets"].as_array().ok_or_else(|| {
            format!(
                "No assets found in release for {}. The release might be empty or the API response changed.",
                repo
            )
        })?;

        tracing::debug!(asset_count = assets.len(), "Found release assets");

        let os_targets: Vec<&str> = match os {
            "linux" => {
                let mut gnu_preferred = true;

                #[cfg(target_os = "linux")]
                {
                    // Atuin's GNU binary is built against glibc 2.35.
                    // If the system's glibc is older, we prefer musl.
                    const MIN_GLIBC_VERSION: (u32, u32) = (2, 35);

                    if let Some((major, minor)) = get_glibc_version() {
                        if (major, minor) < MIN_GLIBC_VERSION {
                            tracing::info!(
                                "System glibc version {}.{} is older than required {}.{}. Prioritizing musl build.",
                                major, minor, MIN_GLIBC_VERSION.0, MIN_GLIBC_VERSION.1
                            );
                            gnu_preferred = false;
                        }
                    } else {
                        tracing::warn!("Could not determine glibc version. Defaulting to musl for safety.");
                        gnu_preferred = false; // Default to safer musl if check fails
                    }
                }

                let default_targets = if gnu_preferred {
                    vec!["unknown-linux-gnu", "unknown-linux-musl"]
                } else {
                    vec!["unknown-linux-musl", "unknown-linux-gnu"]
                };

                match name {
                    "fish" | "helix" => vec!["linux"],
                    _ => default_targets,
                }
            }
            "android" => {
                // Android does not use glibc, so musl is generally the better choice if available.
                 match name {
                    "fish" | "helix" => vec!["linux"],
                    _ => vec!["unknown-linux-musl", "unknown-linux-gnu"],
                }
            }
            "macos" => vec!["apple-darwin"],
            "windows" => vec!["pc-windows-msvc"],
            _ => return Err(format!("Unsupported OS: {}", os)),
        };

        let ext = if os == "windows" {
            "zip"
        } else {
            match name {
                "helix" if os == "linux" => "tar.xz",
                "helix" if os == "macos" => "zip",
                "fish" => "tar.xz",
                _ => "tar.gz",
            }
        };

        for os_target in &os_targets {
            let fragments_to_use = vec![name, arch, *os_target, ext];
            tracing::debug!(fragments = ?fragments_to_use, "Searching for asset");

            for asset in assets {
                let asset_name = asset["name"].as_str().unwrap_or("");
                let lower_name = asset_name.to_lowercase();

                if fragments_to_use
                    .iter()
                    .all(|frag| lower_name.contains(&frag.to_lowercase()))
                {
                    if let Some(url) = asset["browser_download_url"].as_str() {
                        tracing::info!(asset = asset_name, "Found matching release asset");
                        return Ok((url.to_string(), asset_name.to_string()));
                    }
                }
            }
        }

        Err(format!(
            "Could not find a matching release asset for {} on {} {}",
            name, os, arch
        ))
    })
    .await;

    result.map_err(|e: String| anyhow!(e))
}

#[tracing::instrument(skip(reader))]
fn extract_tar_gz<R: Read>(reader: R, target_dir: &Path, binary_name: &str) -> AppResult<()> {
    let tar = GzDecoder::new(reader);
    let mut archive = Archive::new(tar);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        tracing::trace!(entry_path = ?path, "Unpacking archive entry");
        if path.file_name().map_or(false, |n| n == binary_name) {
            entry.unpack(target_dir.join(binary_name))?;
            return Ok(());
        }
    }
    Err(anyhow!(
        "Could not find '{}' in the downloaded .tar.gz archive.",
        binary_name
    ))
}

#[tracing::instrument(skip(reader))]
fn extract_tar_xz<R: Read>(reader: R, target_dir: &Path, binary_name: &str) -> AppResult<()> {
    let tar = XzDecoder::new(reader);
    let mut archive = Archive::new(tar);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?;
        tracing::trace!(entry_path = ?path, "Unpacking archive entry");
        if path.file_name().map_or(false, |n| n == binary_name) {
            entry.unpack(target_dir.join(binary_name))?;
            return Ok(());
        }
    }
    Err(anyhow!(
        "Could not find '{}' in the downloaded .tar.xz archive.",
        binary_name
    ))
}

pub fn extract_archive<R: io::Read>(archive: &mut Archive<R>, target_dir: &Path) -> AppResult<()> {
    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?.to_path_buf();
        tracing::trace!(entry_path = ?path, "Unpacking archive entry");

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

pub fn extract_zip_archive<R: io::Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    target_dir: &Path,
) -> AppResult<()> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if let Some(enclosed_name) = file.enclosed_name() {
            tracing::trace!(entry_path = ?enclosed_name, "Unpacking archive entry");
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
fn extract_zip<R: Read + Seek>(reader: R, target_dir: &Path, binary_name: &str) -> AppResult<()> {
    let mut archive = ZipArchive::new(reader)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if let Some(path) = file.enclosed_name() {
            tracing::trace!(entry_path = ?path, "Unpacking archive entry");
            if path.file_name().map_or(false, |n| n == binary_name) {
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
    }
    Err(anyhow!(
        "Could not find '{}' in the downloaded .zip archive.",
        binary_name
    ))
}

#[cfg(unix)]
#[tracing::instrument(fields(original = %original.display(), link = %link.display()))]
pub fn create_symlink(original: &Path, link: &Path) -> AppResult<()> {
    let link_parent = link.parent().unwrap_or_else(|| Path::new(""));
    let relative_path = pathdiff::diff_paths(original, link_parent)
        .ok_or_else(|| anyhow!("Failed to calculate relative path for symlink"))?;
    tracing::debug!(relative_path = %relative_path.display(), "Calculated relative path for symlink");
    symlink(relative_path, link).context("Failed to create symlink")
}

#[cfg(windows)]
#[tracing::instrument(fields(original = %original.display(), link = %link.display()))]
pub fn create_symlink(original: &Path, link: &Path) -> AppResult<()> {
    let link_parent = link.parent().unwrap_or_else(|| Path::new(""));
    let relative_path = pathdiff::diff_paths(original, link_parent)
        .ok_or_else(|| anyhow!("Failed to calculate relative path for symlink"))?;
    tracing::debug!(relative_path = %relative_path.display(), "Calculated relative path for symlink");

    if original.is_dir() {
        symlink_dir(relative_path, link).context("Failed to create directory symlink")
    } else {
        symlink_file(relative_path, link).context("Failed to create file symlink")
    }
}

/// Extracts only the 'share' directory from a gzipped tarball into a target directory.
/// It strips the top-level directory from the archive, so the contents of 'share'
/// are placed directly in the target directory.
#[tracing::instrument(skip(reader))]
pub fn extract_share_from_tar_gz<R: Read>(reader: R, target_dir: &Path) -> AppResult<()> {
    let tar = GzDecoder::new(reader);
    let mut archive = Archive::new(tar);
    fs::create_dir_all(target_dir)?;

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?;

        // Find paths that are inside a 'share' directory.
        // e.g., "fish-shell-3.7.1/share/completions/git.fish"
        if let Some(share_index) = path.to_str().and_then(|s| s.find("/share/")) {
            // Get the path relative to the inside of the 'share' dir.
            // For the example above, this becomes "completions/git.fish".
            // We add 1 to the index to skip the leading slash.
            let relative_path_str = &path.to_str().unwrap()[share_index + 1..];
            let relative_path = Path::new(relative_path_str);

            // Prepend the target directory to get the final output path.
            let outpath = target_dir.join(relative_path);

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

/// For a symlinked Helix, provisions a local runtime if the user-wide one is missing.
#[tracing::instrument(skip(system_hx_path, env_dir, pb))]
pub fn provision_helix_runtime_for_symlink(
    system_hx_path: &Path,
    env_dir: &Path,
    pb: &ProgressBar,
) -> AppResult<()> {
    // 1. Get Helix version from the system binary.
    let version_output = get_binary_version(system_hx_path, "--version")?;
    let version_tag = parse_helix_version_tag(&version_output)?;
    tracing::debug!(version = %version_tag, "Parsed helix version from symlinked binary");

    // 2. Find the GitHub release asset URL for that specific tag.
    let (download_url, asset_name) = find_github_release_asset_url_by_tag(
        "helix-editor/helix",
        &version_tag,
        env::consts::OS,
        env::consts::ARCH,
        "https://api.github.com",
    )?;

    // 3. Download the archive to a temp file.
    let temp_file = download_to_temp_file_blocking(&download_url, &asset_name, pb)?;

    // 4. Selectively extract ONLY the `runtime` directory.
    let helix_dir = env_dir.join("helix");
    fs::create_dir_all(&helix_dir)?;
    tracing::debug!(path = %helix_dir.display(), "Ensured helix directory exists");

    let file = temp_file.reopen()?;
    if asset_name.ends_with(".tar.xz") {
        let tar = XzDecoder::new(file);
        let mut archive = Archive::new(tar);
        extract_runtime_from_archive(&mut archive, &helix_dir)?;
    } else if asset_name.ends_with(".zip") {
        let mut zip = ZipArchive::new(file)?;
        extract_runtime_from_zip_archive(&mut zip, &helix_dir)?;
    } else {
        return Err(anyhow!(
            "Unsupported archive format for Helix runtime: {}",
            asset_name
        ));
    }

    tracing::info!("Successfully provisioned local Helix runtime.");
    Ok(())
}

/// Executes a binary with a given argument to get its version string.
fn get_binary_version(path: &Path, arg: &str) -> AppResult<String> {
    let output = Command::new(path)
        .arg(arg)
        .output()
        .with_context(|| format!("Failed to execute binary: {}", path.display()))?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to get version from {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8(output.stdout)?)
}

/// Parses the Helix version tag (e.g., "24.03") from the command output.
fn parse_helix_version_tag(version_output: &str) -> AppResult<String> {
    let re = Regex::new(r"helix (\d+\.\d+)")?;
    let caps = re.captures(version_output).ok_or_else(|| {
        anyhow!(
            "Failed to parse Helix version from output: '{}'",
            version_output
        )
    })?;
    Ok(caps.get(1).unwrap().as_str().to_string())
}

/// Downloads a file in a blocking context.
fn download_to_temp_file_blocking(
    url: &str,
    asset_name: &str,
    pb: &ProgressBar,
) -> AppResult<NamedTempFile> {
    pb.set_position(0);

    let mut response = reqwest::blocking::Client::builder()
        .user_agent("isoterm")
        .build()?
        .get(url)
        .send()?
        .error_for_status()?;

    let total_size = response.content_length().unwrap_or(0);

    let download_style = ProgressStyle::with_template(
        "{spinner:.green} {msg}\n{wide_bar:.cyan/blue} {bytes}/{total_bytes} ({eta})",
    )?
    .progress_chars("#>-");

    pb.set_style(download_style);
    pb.set_length(total_size);
    pb.set_message(format!("Downloading {}", style(asset_name).cyan()));

    let mut temp_file = NamedTempFile::new()?;
    let mut buffer = [0; 8192]; // 8KB buffer

    loop {
        let bytes_read = response.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        temp_file.write_all(&buffer[..bytes_read])?;
        pb.inc(bytes_read as u64);
    }

    Ok(temp_file)
}

/// Finds a GitHub release asset URL for a specific version tag.
#[tracing::instrument(fields(repo = repo, tag = tag, os = os, arch = arch))]
fn find_github_release_asset_url_by_tag(
    repo: &str,
    tag: &str,
    os: &str,
    arch: &str,
    base_url: &str,
) -> AppResult<(String, String)> {
    let repo_url = format!("{}/repos/{}/releases/tags/{}", base_url, repo, tag);
    tracing::debug!(url = %repo_url, "Fetching release by tag from GitHub API");

    let response: Value = reqwest::blocking::Client::new()
        .get(&repo_url)
        .header("User-Agent", "isoterm")
        .send()?
        .error_for_status()?
        .json()?;

    let assets = response["assets"].as_array().ok_or_else(|| {
        anyhow!(
            "No assets found in release {} for {}. The release might be empty or the API response changed.",
            tag, repo
        )
    })?;

    let os_targets: Vec<&str> = match os {
        "linux" => vec!["linux"],
        "macos" => vec!["apple-darwin"],
        "windows" => vec!["pc-windows-msvc"],
        _ => return Err(anyhow!("Unsupported OS for Helix runtime: {}", os)),
    };

    let ext = match os {
        "linux" => "tar.xz",
        "macos" | "windows" => "zip",
        _ => return Err(anyhow!("Unsupported OS for Helix runtime: {}", os)),
    };

    for os_target in &os_targets {
        let fragments_to_use = vec!["helix", tag, arch, *os_target, ext];
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
        "Could not find a matching Helix runtime release asset for tag {} on {} {}",
        tag,
        os,
        arch
    ))
}

/// Selectively extracts only the `runtime/` directory from a tar archive.
pub fn extract_runtime_from_archive<R: io::Read>(
    archive: &mut Archive<R>,
    target_dir: &Path,
) -> AppResult<()> {
    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?;

        // Check if the entry is within the 'runtime' directory.
        // e.g., "helix-24.03-x86_64-linux/runtime/themes/catppuccin.toml"
        if let Some(runtime_index) = path.to_str().and_then(|s| s.find("/runtime/")) {
            // Get the path relative to the top-level folder inside the archive.
            // e.g., "runtime/themes/catppuccin.toml"
            let relative_path_str = &path.to_str().unwrap()[runtime_index + 1..];
            let relative_path = Path::new(relative_path_str);

            let outpath = target_dir.join(relative_path);

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

/// Selectively extracts only the `runtime/` directory from a zip archive.
pub fn extract_runtime_from_zip_archive<R: io::Read + io::Seek>(
    archive: &mut ZipArchive<R>,
    target_dir: &Path,
) -> AppResult<()> {
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if let Some(enclosed_name) = file.enclosed_name() {
            if let Some(runtime_index) = enclosed_name.to_str().and_then(|s| s.find("/runtime/")) {
                let relative_path_str = &enclosed_name.to_str().unwrap()[runtime_index + 1..];
                let relative_path = Path::new(relative_path_str);
                let outpath = target_dir.join(relative_path);

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
    }
    Ok(())
}
