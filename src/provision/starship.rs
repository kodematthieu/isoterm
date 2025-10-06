use super::Tool;

pub struct Starship;

impl Tool for Starship {
    fn name(&self) -> &'static str {
        "starship"
    }

    fn repo(&self) -> &'static str {
        "starship/starship"
    }

    fn binary_name(&self) -> &'static str {
        "starship"
    }
}