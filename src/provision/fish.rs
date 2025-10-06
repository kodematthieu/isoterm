use super::{
    ProvisionContext, Tool, create_symlink, download_to_temp_file, extract_archive,
    find_github_release_asset_url, provision_source_share,
};
use crate::error::AppResult;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::fs;
use tar::Archive;
use xz2::read::XzDecoder;

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

        let tar = XzDecoder::new(file);
        let mut archive = Archive::new(tar);
        extract_archive(&mut archive, &fish_runtime_dir)?;

        let binary_path_in_archive = fish_runtime_dir.join("bin").join(self.binary_name());
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
}
