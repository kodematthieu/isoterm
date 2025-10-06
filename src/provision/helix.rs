use super::{
    ProvisionContext, Tool, download_and_install_archive, provision_helix_runtime_for_symlink,
};
use crate::error::AppResult;
use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use shellexpand;
use std::path::Path;
use tokio::task;

pub struct Helix;

impl Tool for Helix {
    fn name(&self) -> &'static str {
        "helix"
    }

    fn repo(&self) -> &'static str {
        "helix-editor/helix"
    }

    fn binary_name(&self) -> &'static str {
        "hx"
    }

    fn path_in_archive(&self) -> Option<&'static str> {
        Some("hx")
    }

    #[tracing::instrument(skip(self, context, pb, spinner_style), fields(tool = self.name()))]
    async fn provision_from_source(
        &self,
        context: &ProvisionContext,
        pb: &ProgressBar,
        spinner_style: &ProgressStyle,
    ) -> AppResult<()> {
        download_and_install_archive(
            &context.env_dir,
            self.name(),
            self.repo(),
            self.binary_name(),
            self.path_in_archive().unwrap_or_default(),
            pb,
            spinner_style,
            &context.client,
        )
        .await
    }

    #[tracing::instrument(skip(self, context, pb, system_path), fields(tool = self.name()))]
    async fn post_symlink_hook(
        &self,
        context: &ProvisionContext,
        pb: &ProgressBar,
        system_path: &Path,
    ) -> AppResult<()> {
        let user_helix_runtime_dir = shellexpand::tilde("~/.config/helix/runtime").to_string();
        if !Path::new(&user_helix_runtime_dir).exists() {
            tracing::debug!("User-wide helix runtime not found. Provisioning a local one.");
            pb.println(" › Detected Helix symlink without a user-wide runtime. Provisioning a local runtime to match the system binary's version...".to_string());

            let system_path_clone = system_path.to_path_buf();
            let env_dir_clone = context.env_dir.to_path_buf();
            let pb_clone = pb.clone();

            // This part is synchronous (blocking HTTP calls, file I/O), so it's
            // best to run it in a blocking-safe thread to avoid stalling the async runtime.
            task::spawn_blocking(move || {
                provision_helix_runtime_for_symlink(&system_path_clone, &env_dir_clone, &pb_clone)
            })
            .await
            .context("Task for provisioning helix runtime panicked")??;
        }
        Ok(())
    }
}
