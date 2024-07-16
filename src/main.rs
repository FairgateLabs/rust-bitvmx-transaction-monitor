use anyhow::{Context, Ok, Result};
use clap::Parser;
use log::info;
use rust_bitcoin_tx_monitor::{
    args::Args,
    stores::{bitcoin_store::BitcoinStore, bitvmx_store::BitvmxStore},
};

fn main() -> Result<()> {
    dotenv::dotenv().context("There was an error loading .env file")?;
    env_logger::init();

    let args = Args::parse();

    info!("{:#?}", args);

    //let mut monitor = Monitor::new(args.bitcoin_indexer_db_url, args.operation_db_url)?;

    // let bitcoin_store: BitcoinStore = BitcoinStore::new(bitcoin_store)?;
    // let operation_store = OperationStore::new(operation_store)?;

    // monitor
    //     .run()
    //
    //     .expect("Something went wrong in the Monitor");
    Ok(())
}
