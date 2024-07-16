use anyhow::{Context, Result};
use mockall::automock;

use crate::stores::{bitcoin_store::BitcoinApi, bitvmx_store::BitvmxStore};

pub struct Monitor<B: BitcoinApi> {
    pub bitcoin_store: B,
    pub operation_store: BitvmxStore,
}
pub trait Runner {
    fn run(&mut self) -> Result<()>;
}

#[automock]
impl<B: BitcoinApi> Runner for Monitor<B> {
    fn run(&mut self) -> Result<()> {
        //Get current block from Bitcoin Indexer
        // let current_height = self
        //     .bitcoin_store
        //     .get_block_count()
        //     .context("Failed to retrieve current block")?;

        // // Get operations that have already started
        // let operations = self
        //     .operation_store
        //     .get_operations(current_height)
        //     .context("Failed to retrieve operations")?;

        // // count existing operations get all thansaction that meet next rules:
        // for operation in operations {
        //     for tx in operation.txs {
        //         if tx.tx_was_seen && tx.block_confirmations > 6 {
        //             break;
        //         }

        //         let tx_exists = self.bitcoin_store.tx_exists(&tx.txid)?;

        //         self.operation_store
        //             .update_tx_operation(operation.id, tx.txid, tx_exists, current_height)
        //             .with_context(|| {
        //                 format!(
        //                     "Fail updating operation id {} for tx id {}",
        //                     operation.id, tx.txid
        //                 )
        //             })?;
        //     }
        // }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::stores::bitcoin_store::MockBitcoinStore;

    use super::*;

    #[test]
    fn monitor_test() -> Result<(), anyhow::Error> {
        let mut mock_bitcoin_store = MockBitcoinStore::new();

        mock_bitcoin_store
            .expect_get_block_count()
            .returning(|| Ok(11));

        // let mut operator = bitvmxStore::new(String::from(""));

        // let mut monitor = Monitor {
        //     bitcoin_store: mock_bitcoin_store,
        //     operation_store: operator,
        // };
        // // assert_eq!(a, 11);
        // monitor.run()?;

        Ok(())
    }
}
