use clap::{command, Parser};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]

pub struct Args {
    /// Bitcoin indexer database URL
    #[arg(short, long)]
    pub bitcoin_indexer_db_url: String,

    /// Bitvmx instances file path
    #[arg(short, long)]
    pub bitvmx_file_path: String,
}
