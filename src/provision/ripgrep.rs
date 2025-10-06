use super::Tool;

pub struct Ripgrep;

impl Tool for Ripgrep {
    fn name(&self) -> &'static str {
        "ripgrep"
    }

    fn repo(&self) -> &'static str {
        "BurntSushi/ripgrep"
    }

    fn binary_name(&self) -> &'static str {
        "rg"
    }
}