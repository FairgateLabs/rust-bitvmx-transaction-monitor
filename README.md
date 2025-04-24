# BitVMX Transaction Monitor

This monitor tracks various Bitcoin transactions. It connects to an Indexer and provides real-time updates on transaction confirmations for different transaction types.


**Transaction Monitoring Types**:
  - **Transactions**: Monitor a set of transactions
  - **RSK Pegin Transactions**: Detect and track RSK pegin transactions
  - **UTXO Spending**: Monitor when specific UTXOs are spent
  - **New Block**: Get notifications when a new block is added to the blockchain
  
## Features

- **Status Updates**: Get notifications when monitored transactions receive new confirmations
- **Blockchain Synchronization**: Automatically syncs with the Bitcoin blockchain on every tick.
- **Persistence**: Monitoring state is preserved across restarts

## Architecture

The monitor consists of three main components:

1. **Indexer**: External library that connects to a Bitcoin node and indexes blockchain data
2. **Monitor Store**: Persists monitoring transactions and news
3. **Monitor**: Core logic that processes blocks, detects transactions, and manages news updates

## Configuration

The monitor is configured through a YAML file. Here's an example configuration (`development.yaml`):

## Installation

Clone the repository:

```bash
$ git clone git@github.com:FairgateLabs/rust-bitvmx-transaction-monitor
``` 

## Run Tests
```rust
$ cargo test
```
## API Endpoints

The `Monitor` struct implements the `MonitorApi` trait with the following methods:

### Core Functionality

- **`is_ready()`**: Checks if the monitor is fully synced with the blockchain.
  - Returns `true` if synced, `false` if still syncing, or an error.

- **`tick()`**: Processes new blocks, updates transaction statuses, and generates news.
  - Should be called periodically to keep the monitor in sync with the blockchain.

- **`monitor(data: TypesToMonitor)`**: Registers a new transaction or entity to be monitored.
  - Supports multiple transaction types (Transactions, RSK Pegin, UTXO Spending, New Block).

- **`get_tx_status(tx_id: &Txid)`**: Retrieves the current status of a monitored transaction.
  - Includes confirmation count, block information, and transaction details.

### News Management

- **`get_news()`**: Retrieves all pending news items for monitored transactions.
  - News items include confirmation updates and status changes.

- **`ack_news(data: AckMonitorNews)`**: Marks specific news items as read/processed.
  - Prevents the same news from being returned in subsequent calls.

### Transaction Monitoring

- **`get_monitors()`**: Retrieves all currently active transaction monitors.
  - Returns a list of all transactions and entities being monitored.

- **`deactivate_monitor(data: TypesToMonitor)`**: Temporarily deactivates monitoring for a specific transaction or entity.
  - The monitor remains in the store but is marked as inactive and won't generate news.

- **`cancel(data: TypesToMonitor)`**: Stops monitoring a specific transaction or entity.
  - Transaction news remains in the store but no new updates will be generated. 

### Blockchain Information

- **`get_confirmation_threshold()`**: Returns the configured confirmation threshold for transactions.
  - Determines how many confirmations are required before a transaction is considered final.

- **`get_monitor_height()`**: Returns the current height the monitor has processed up to.
  - Useful for determining sync status.
