# BitVMX Transaction Monitor

The BitVMX Transaction Monitor is a comprehensive tool for tracking and managing different types of transaction monitors. It connects with an Indexer to deliver real-time updates on transaction confirmations for various monitor types, such as UTXO transactions, RSK Peg-In transactions, and block monitoring.

## ⚠️ Disclaimer

This library is currently under development and may not be fully stable.
It is not production-ready, has not been audited, and future updates may introduce breaking changes without preserving backward compatibility.

## Key Features

- 📡 **Real-Time Status Updates**: Receive immediate notifications when monitored transactions receive new confirmations.
- 🔄 **Automatic Blockchain Synchronization**: Seamlessly syncs with the Bitcoin blockchain at regular intervals.
- 💾 **State Persistence**: Maintains monitoring state across system restarts for uninterrupted tracking.

## System Architecture

The monitor is built on three primary components:

1. **Indexer**: An external library that connects to a Bitcoin node to index blockchain data.
2. **Monitor Store**: A storage system that retains monitoring transactions and news updates.
3. **Monitor**: The core component that processes blocks, detects transactions, and manages news updates.

## Configuration

Configuration is managed through a YAML file. An example configuration file, `monitor_config.yaml`, is located in the `config/` directory.

## Methods

The `Monitor` struct implements the `MonitorApi` trait, offering the following methods:

### Core Operations

- **`is_ready()`**: Checks if the monitor is fully synchronized with the blockchain.
  

- **`tick()`**: Executes a monitoring cycle, processing new blocks, updating transaction statuses, and generating news. Should be called periodically to ensure blockchain synchronization.

### News Management

- **`get_news()`**: Gathers all pending news items related to monitored transactions. Includes confirmation updates and status changes.

- **`ack_news(data: AckMonitorNews)`**: Marks specific news items as processed. Prevents the same news from being returned in future queries.

### Monitors Management

- **`monitor(data: TypesToMonitor)`**: Initiates the monitoring process for a new transaction or entity.  Capable of handling multiple monitor types, such as Bitcoin Transactions, RSK Pegin Transactions, UTXO Spending, New Block notifications.
 
- **`cancel(data: TypesToMonitor)`**: Completely stops monitoring a specific transaction or entity. Existing transaction news is retained, but no further updates will be generated.

### Blockchain Information

- **`get_confirmation_threshold()`**: Retrieves the configured confirmation threshold for transactions.
  - Indicates the number of confirmations required for a transaction to be considered final.

- **`get_monitor_height()`**: Provides the current block height processed by the monitor.
  - Useful for evaluating synchronization status.

- **`get_tx_status(tx_id: &Txid)`**: Retrieves the current status of a monitored transaction. Provides details such as confirmation count, block information, and transaction specifics.

## Usage

Here's how you can use the `Monitor` struct and its methods in your application:

```rust
  // Initialize the monitor with necessary components
  let monitor = Monitor::new(indexer, bitvmx_store, settings)?;

  // Check if the monitor is fully synchronized with the blockchain
  match monitor.is_ready() {
      Ok(true) => println!("Monitor is fully synchronized."),
      Ok(false) => println!("Monitor is still syncing."),
      Err(e) => eprintln!("Error checking monitor readiness: {:?}", e),
  }

  // Regularly tick the monitor to process new blocks and update statuses
  monitor.tick();

  // Retrieve all pending news items related to monitored transactions
  let news = monitor.get_news()

  // Acknowledge specific news items to prevent duplicate notifications
  let ack_data = /* create your AckMonitorNews instance */;
  monitor.ack_news(ack_data) 

  // Start monitoring a new transaction or entity
  let monitor_data = /* create your TypesToMonitor instance */;
  monitor.monitor(monitor_data)

  // Stop monitoring a specific transaction or entity
  let cancel_data = /* create your TypesToMonitor instance */;
  monitor.cancel(cancel_data) 

  // Retrieve the confirmation count needed for a transaction to achieve finality
  let threshold = monitor.get_confirmation_threshold();
  println!("Confirmation threshold: {}", threshold);

  // Get the current block height processed by the monitor
  match monitor.get_monitor_height() {
      Ok(height) => println!("Current monitor height: {}", height),
      Err(e) => eprintln!("Error retrieving monitor height: {:?}", e),
  }

  // Retrieve the current status of a monitored transaction
  let tx_id = /* your transaction ID */;
  match monitor.get_tx_status(&tx_id) {
      Ok(status) => println!("Transaction status: {:?}", status),
      Err(e) => eprintln!("Error retrieving transaction status: {:?}", e),
  }
  ```

## Development Setup

1. Clone the repository.
2. Install dependencies using `cargo build`.
3. Run tests with `cargo test -- --ignored`.

## Contributing 
Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License
This project is licensed under the MIT License.
