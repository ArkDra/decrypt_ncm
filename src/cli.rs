use clap::Parser;
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Decrypt NCM files into their original MP3 or FLAC format",
    after_help = "Directory scans are not recursive and ignore symbolic links.\nExit codes: 0 = success, 1 = conversion or I/O failure, 2 = invalid arguments."
)]
pub struct Args {
    /// NCM file or directory to process
    #[arg(value_name = "FILE_OR_DIRECTORY")]
    pub input: PathBuf,

    /// Save converted files in this directory (created if needed)
    #[arg(short = 'o', long, value_name = "DIRECTORY")]
    pub output_dir: Option<PathBuf>,

    /// Replace existing output files
    #[arg(long)]
    pub overwrite: bool,

    /// Maximum number of concurrent file conversions
    #[arg(short = 'j', long, value_name = "COUNT", default_value = "2")]
    pub jobs: NonZeroUsize,
}
