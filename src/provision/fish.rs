use super::{
    provision_fish_runtime_for_symlink, provision_source_share, AssetSpec, ProvisionContext, Tool,
};
use crate::error::AppResult;
use std::borrow::Cow;
use anyhow::Context;
use indicatif::ProgressBar;
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

    fn asset_spec<'a>(&self, os: &'a str, arch: &'a str) -> AppResult<AssetSpec<'a>> {
        let os_keyword = match os {
            "linux" | "android" => "linux",
            _ => os,
        };

        Ok(AssetSpec {
            os_keywords: vec![os_keyword],
            arch_keyword: arch,
            extension: "tar.xz",
            name_keyword: Cow::from(self.name()),
        })
    }

    // Fish requires a `FullArchive` extraction, but also needs to ensure the `share`
    // directory is present, which is not always in the release archive. So we
    // override the default `provision_from_source`.
    #[tracing::instrument(skip(self, context, pb, spinner_style), fields(tool = self.name()))]
    async fn provision_from_source(
        &self,
        context: &ProvisionContext,
        pb: &ProgressBar,
        spinner_style: &super::ProgressStyle,
    ) -> AppResult<()> {
        super::provision_from_github_release(
            context,
            self,
            super::ExtractionStrategy::FullArchive {
                path_in_archive: "fish",
            },
            pb,
            spinner_style,
        )
        .await?;

        // This is necessary because some release archives (like for macOS) don't
        // include the 'share' directory, which has completions, etc.
        let fish_runtime_dir = context.env_dir.join("fish_runtime");
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
