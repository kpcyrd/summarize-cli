use clap::{ArgAction, Parser};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(version)]
pub struct Args {
    /// Increase logging output (can be used multiple times)
    #[arg(short, long, global = true, action(ArgAction::Count))]
    pub verbose: u8,
    /// The file to summarize, defaults to stdin
    #[arg(default_value = "-")]
    pub path: PathBuf,
    /// The number of bits used for context_size (2^n)
    #[arg(short = 'c', default_value = "11")]
    pub context_size: u32,
    /// Load llama model from provided path
    #[arg(short = 'p', long)]
    pub model_path: Option<PathBuf>,
}
