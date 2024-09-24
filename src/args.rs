use bitcoin_indexer::types::BlockHeight;
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

    #[arg(long, short = 'd')]
    pub db_file_path: Option<String>,

    /// Bitcoin node rpc url
    #[arg(long, short = 'n')]
    pub node_rpc_url: Option<String>,

    /// Bitcoin height to start indexing from
    #[arg(long, short = 'c')]
    pub checkpoint_height: Option<BlockHeight>,
}
