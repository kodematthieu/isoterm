use crate::error::AppResult;
use crate::provision::{AssetSpec, Tool};
use std::borrow::Cow;

pub struct Fastfetch;

impl Tool for Fastfetch {
    fn name(&self) -> &'static str {
        "fastfetch"
    }

    fn repo(&self) -> &'static str {
        "fastfetch-cli/fastfetch"
    }

    fn binary_name(&self) -> &'static str {
        "fastfetch"
    }

    fn asset_spec<'a>(&self, os: &'a str, arch: &'a str) -> AppResult<AssetSpec<'a>> {
        let arch_keyword = match arch {
            "x86_64" => "amd64",
            "aarch64" => "aarch64",
            _ => arch,
        };

        let os_keyword = match os {
            "macos" => "macos",
            _ => "linux", // Covers linux, android
        };

        // Fastfetch assets are uniquely named, e.g., "fastfetch-linux-amd64.tar.gz"
        // So, we construct the name_keyword to match this pattern.
        let name_keyword = format!("{}-{}-{}", self.name(), os_keyword, arch_keyword);

        Ok(AssetSpec {
            os_keywords: vec![os_keyword],
            arch_keyword,
            extension: "tar.gz",
            name_keyword: Cow::from(name_keyword),
        })
    }
}
