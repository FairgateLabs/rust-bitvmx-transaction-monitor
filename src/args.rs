use clap::{command, Parser};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]

pub struct Args {
    /// Bitcoin indexer database URL
    #[arg(long, short = 'v')]
    pub bitcoin_indexer_db_url: Option<String>,

    /// Bitvmx instances file path
    #[arg(long, short = 'f')]
    pub bitvmx_file_path: Option<String>,
}
