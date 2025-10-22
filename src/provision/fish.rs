use super::{
    provision_fish_runtime_for_symlink, ArchiveType, ProvisionContext, Tool, create_symlink,
    download_to_temp_file, extract_full_archive, find_github_release_asset_url,
    provision_source_share,
};
use crate::error::AppResult;
use anyhow::Context;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use std::path::Path;
use tokio::task;

pub struct Fish;

impl Tool for Fish {
    fn name(&self) -> &'static str {
        "fish"
    }

    fn repo(&self) -> &'static str {
        "fish-shell/fish-shell"
    }

    fn binary_name(&self) -> &'static str {
        "fish"
    }

    #[tracing::instrument(skip(self, context, pb, spinner_style), fields(tool = self.name()))]
    async fn provision_from_source(
        &self,
        context: &ProvisionContext,
        pb: &ProgressBar,
        spinner_style: &ProgressStyle,
    ) -> AppResult<()> {
        // --- Fish-specific download and extraction ---
        pb.set_message(format!("Downloading {}...", style(self.name()).bold()));
        let (download_url, asset_name) = find_github_release_asset_url(
            self.name(),
            self.repo(),
            "https://api.github.com",
            env::consts::OS,
            env::consts::ARCH,
            &context.client,
        )
        .await?;
        let temp_file =
            download_to_temp_file(&download_url, &asset_name, pb, &context.client).await?;
        let file = temp_file.reopen()?;

        pb.set_style(spinner_style.clone());
        pb.set_message(format!(
            "Extracting archive for {}...",
            style(self.name()).bold()
        ));

        let fish_runtime_dir = context.env_dir.join("fish_runtime");
        fs::create_dir_all(&fish_runtime_dir)?;

        let archive_type = ArchiveType::from_asset_name(&asset_name)?;
        extract_full_archive(file, archive_type, &fish_runtime_dir)?;

        let binary_path_in_archive = fish_runtime_dir.join(self.binary_name());
        let tool_path_in_env = context.env_dir.join("bin").join(self.binary_name());
        create_symlink(&binary_path_in_archive, &tool_path_in_env)?;

        // --- Fish-specific 'share' directory provisioning ---
        // This is necessary because some release archives (like for macOS) don't
        // include the 'share' directory, which contains completions and other essential files.
        if !fish_runtime_dir.join("share").exists() {
            provision_source_share(
                &fish_runtime_dir,
                self.name(),
                self.repo(),
                pb,
                &context.client,
            )
            .await?;
        } else {
            tracing::debug!("'share' directory already exists, skipping download.");
        }

        Ok(())
    }

    #[tracing::instrument(skip(self, context, pb, system_path), fields(tool = self.name()))]
    async fn post_symlink_hook(
        &self,
        context: &ProvisionContext,
        pb: &ProgressBar,
        system_path: &Path,
    ) -> AppResult<()> {
        pb.println(
            " › Detected Fish symlink. Ensuring a version-matched runtime is present...".to_string(),
        );

        let system_path_clone = system_path.to_path_buf();
        let env_dir_clone = context.env_dir.to_path_buf();
        let pb_clone = pb.clone();

        // This part is synchronous (blocking HTTP calls, file I/O), so it's
        // best to run it in a blocking-safe thread to avoid stalling the async runtime.
        task::spawn_blocking(move || {
            provision_fish_runtime_for_symlink(&system_path_clone, &env_dir_clone, &pb_clone)
        })
        .await
        .context("Task for provisioning fish runtime panicked")??;

        Ok(())
    }
}
