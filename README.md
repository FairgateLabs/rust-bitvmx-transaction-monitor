# BitVMX Transaction Monitor

The BitVMX Transaction Monitor is a comprehensive tool for tracking and managing different types of transaction monitors. It connects with an Indexer to deliver real-time updates on transaction confirmations for various monitor types, such as UTXO transactions, RSK Peg-In transactions, and block monitoring.

## ⚠️ Disclaimer

This library is currently under development and may not be fully stable.
It is not production-ready, has not been audited, and future updates may introduce breaking changes without preserving backward compatibility.

## Key Features

- 📡 **Real-Time Status Updates**: Receive immediate notifications when monitored transactions receive new confirmations.
- 🔄 **Automatic Blockchain Synchronization**: Seamlessly syncs with the Bitcoin blockchain at regular intervals.
- 💾 **State Persistence**: Maintains monitoring state across system restarts for uninterrupted tracking.

### ⚠️ SegWit Requirement For Reliable Tracking

The transaction monitor relies on transaction IDs (txids) to follow confirmations. Legacy (non-SegWit) transactions have malleable txids, so a re-mined transaction could receive a different txid and the monitor would lose track of it—especially because the monitor reports transactions once they reach at least one confirmation and does not enforce SegWit-only inputs. BitVMX currently uses Pay-to-Taproot (P2TR), which is SegWit-based and therefore not susceptible to third-party malleability. If you intend to track legacy transactions, you must either ensure they are SegWit variants (P2WPKH, P2WSH, P2TR, etc.) or reject non-SegWit monitors to avoid missing confirmations.

## System Architecture

The monitor is built on three primary components:

1. **Indexer**: An external library that connects to a Bitcoin node to index blockchain data.
2. **Monitor Store**: A storage system that retains monitoring transactions and news updates.
3. **Monitor**: The core component that processes blocks, detects transactions, and manages news updates.

## Configuration

Configuration is managed through a YAML file. An example configuration file, `monitor_config.yaml`, is located in the `config/` directory.

At minimum, your config should include `settings.indexer_settings.confirmation_threshold` (required by the indexer settings schema). Example:

```yaml
bitcoin:
  network: regtest
  url: http://127.0.0.1:18443
  username: foo
  password: rpcpassword
  wallet: test_wallet

settings:
  max_monitoring_confirmations: 100
  indexer_settings:
    checkpoint_height: 10
    confirmation_threshold: 6

storage:
  path: data
```

## Methods

The `Monitor` struct provides the following public methods:

### Core Operations

- **`is_ready()`**: Checks if the monitor is fully synchronized with the blockchain.
  

- **`tick()`**: Executes a monitoring cycle, processing new blocks, updating transaction statuses, and generating news. Should be called periodically to ensure blockchain synchronization.

### News Management

- **`get_news()`**: Gathers all pending news items related to monitored transactions. Includes confirmation updates and status changes.

- **`ack_news(data: AckMonitorNews)`**: Marks specific news items as processed. Prevents the same news from being returned in future queries.

### Monitors Management

- **`monitor(data: TypesToMonitor)`**: Initiates the monitoring process for a new transaction or entity. Capable of handling multiple monitor types:
  - **Transactions**: Monitor specific transactions by their transaction IDs. Supports optional confirmation triggers.
  - **RskPegin**: Monitor all RSK pegin transactions automatically. A single monitor detects all RSK pegin transactions in new blocks.
  - **SpendingUTXOTransaction**: Monitor when a specific UTXO (transaction output) is spent. Automatically detects the spending transaction.
  - **NewBlock**: Monitor new blocks being added to the chain. Provides notifications for each new block.
 
- **`cancel(data: TypesToMonitor)`**: Completely stops monitoring a specific transaction or entity. Existing transaction news is retained, but no further updates will be generated.

### Blockchain Information

- **`get_monitor_height()`**: Provides the current block height processed by the monitor.
  - Useful for evaluating synchronization status.

- **`get_tx_status(tx_id: &Txid)`**: Retrieves the current status of a monitored transaction. Provides details such as confirmation count, block information, and transaction specifics.

## Usage

Here's how you can use the `Monitor` struct and its methods in your application:

```rust
  use bitvmx_transaction_monitor::{
      config::MonitorConfig,
      monitor::Monitor,
  };
  use bitvmx_settings::settings;
  use std::rc::Rc;
  use storage_backend::storage::Storage;
  use storage_backend::storage_config::StorageConfig;

  // Load configuration from YAML file
  let config = settings::load_config_file::<MonitorConfig>(Some(
      "config/monitor_config.yaml".to_string(),
  ))?;

  // Create storage from configuration
  let storage = Rc::new(Storage::new(&config.storage)?);

  // Initialize the monitor using new_with_paths
  // This method creates the indexer and store internally
  let monitor = Monitor::new_with_paths(
      &config.bitcoin,
      storage,
      config.settings,
  )?;

  // Check if the monitor is fully synchronized with the blockchain
  match monitor.is_ready() {
      Ok(true) => println!("Monitor is fully synchronized."),
      Ok(false) => println!("Monitor is still syncing."),
      Err(e) => eprintln!("Error checking monitor readiness: {:?}", e),
  }

  // Start monitoring different types of transactions
  use bitvmx_transaction_monitor::types::TypesToMonitor;
  
  // Monitor a specific transaction
  let tx_id = /* your transaction ID */;
  monitor.monitor(TypesToMonitor::Transactions(
      vec![tx_id],
      "my_context".to_string(),
      Some(6), // Optional: only send news when 6+ confirmations
  ))?;
  
  // Monitor when a specific UTXO is spent
  let funding_tx_id = /* funding transaction ID */;
  let vout_index = 0; // output index
  monitor.monitor(TypesToMonitor::SpendingUTXOTransaction(
      funding_tx_id,
      vout_index,
      "utxo_context".to_string(),
      None, // No confirmation trigger
  ))?;
  
  // Monitor all RSK pegin transactions
  monitor.monitor(TypesToMonitor::RskPegin(Some(1)))?;
  
  // Monitor new blocks
  monitor.monitor(TypesToMonitor::NewBlock)?;

  // Regularly tick the monitor to process new blocks and update statuses
  // This should be called in a loop or scheduled task
  monitor.tick()?;

  // Retrieve all pending news items related to monitored transactions
  let news = monitor.get_news()?;
  for news_item in news {
      match news_item {
          MonitorNews::Transaction(tx_id, status, context) => {
              println!("Transaction {} has {} confirmations", tx_id, status.confirmations);
          }
          MonitorNews::SpendingUTXOTransaction(tx_id, vout, status, context) => {
              println!("UTXO {}:{} was spent in transaction {}", tx_id, vout, status.tx_id);
          }
          MonitorNews::RskPeginTransaction(tx_id, status) => {
              println!("RSK pegin transaction {} detected", tx_id);
          }
          MonitorNews::NewBlock(height, hash) => {
              println!("New block at height {}: {}", height, hash);
          }
      }
  }

  // Acknowledge specific news items to prevent duplicate notifications
  use bitvmx_transaction_monitor::types::AckMonitorNews;
  monitor.ack_news(AckMonitorNews::Transaction(tx_id, "my_context".to_string()))?;

  // Stop monitoring a specific transaction or entity
  monitor.cancel(TypesToMonitor::Transactions(
      vec![tx_id],
      "my_context".to_string(),
      None,
  ))?;

  // Get the current block height processed by the monitor
  match monitor.get_monitor_height() {
      Ok(height) => println!("Current monitor height: {}", height),
      Err(e) => eprintln!("Error retrieving monitor height: {:?}", e),
  }

  // Retrieve the current status of a monitored transaction
  match monitor.get_tx_status(&tx_id) {
      Ok(status) => println!("Transaction status: {:?}", status),
      Err(e) => eprintln!("Error retrieving transaction status: {:?}", e),
  }
  ```

## Confirmation Triggers

The monitor supports optional confirmation triggers for transaction monitoring:

- **With trigger**: When a confirmation trigger is set (e.g., `Some(6)`), news is sent **once** when the transaction reaches or exceeds that number of confirmations. This is useful for critical transactions that require a specific confirmation depth.

- **Without trigger**: When no trigger is set (`None`), news is sent for **every block** until the transaction reaches `max_monitoring_confirmations`. This provides continuous updates on confirmation progress.

**Important**: The confirmation trigger must be less than `max_monitoring_confirmations`, otherwise an error will be returned.

## Auto-Deactivation

Monitors are automatically deactivated when transactions reach `max_monitoring_confirmations`. This prevents unnecessary processing and storage overhead. Once deactivated, no further news updates will be generated for that transaction.

## Development Setup

1. Clone the repository.
2. Install dependencies using `cargo build`.
3. Run tests with `cargo test -- --test-threads=1`.

## Contributing 
Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) file for details.

---

## 🧩 Part of the BitVMX Ecosystem

This repository is a component of the **BitVMX Ecosystem**, an open platform for disputable computation secured by Bitcoin.  
You can find the index of all BitVMX open-source components at [**FairgateLabs/BitVMX**](https://github.com/FairgateLabs/BitVMX).

---

