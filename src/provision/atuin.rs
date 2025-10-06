use super::Tool;

pub struct Atuin;

impl Tool for Atuin {
    fn name(&self) -> &'static str {
        "atuin"
    }

    fn repo(&self) -> &'static str {
        "atuinsh/atuin"
    }

    fn binary_name(&self) -> &'static str {
        "atuin"
    }
}
