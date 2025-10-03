use clap::Parser;

/// A tool to create isolated, non-destructive shell environments.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// The directory where the environment will be created.
    #[arg(default_value = "~/.local_shell")]
    pub dest_dir: String,

    /// Enable verbose logging. Use -v for info, -vv for debug.
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
}