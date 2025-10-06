use super::Tool;

pub struct Zoxide;

impl Tool for Zoxide {
    fn name(&self) -> &'static str {
        "zoxide"
    }

    fn repo(&self) -> &'static str {
        "ajeetdsouza/zoxide"
    }

    fn binary_name(&self) -> &'static str {
        "zoxide"
    }
}
