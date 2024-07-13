use clap::{command, Parser};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]

pub struct Args {
    /// Bitcoin indexer database URL
    #[arg(short, long)]
    pub bitcoin_indexer_db_url: String,

    /// Instance Operation database URL
    #[arg(short, long)]
    pub operation_db_url: String,
}
