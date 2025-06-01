use clap::Parser;
use std::path::PathBuf;


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct CliArgs {
    /// The path to scan
    #[arg()] // Positional argument
    pub path: PathBuf,

    /// Output results as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Suppress progress updates and all output except final result
    #[arg(short, long)]
    pub quiet: bool,

    /// Show detailed error information
    #[arg(short, long)]
    pub verbose: bool,

    /// Set concurrent task limit
    #[arg(short, long, value_name = "NUM")]
    pub threads: Option<usize>,

    /// Skip hidden files and directories
    #[arg(long)]
    pub no_hidden: bool,

    /// Follow symbolic links
    #[arg(long)]
    pub follow_symlinks: bool,

    /// Maximum scan duration in seconds
    #[arg(long, value_name = "SECONDS")]
    pub timeout: Option<u64>,

    /// Regex pattern to filter files
    #[arg(short, long, value_name = "PATTERN")]
    pub pattern: Option<String>,
}


pub fn parse_args() -> CliArgs {
    CliArgs::parse()
}
