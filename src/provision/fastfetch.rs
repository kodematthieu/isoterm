use crate::provision::Tool;

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
}
