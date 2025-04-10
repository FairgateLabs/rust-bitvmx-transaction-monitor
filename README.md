# BitVMX Transaction Monitor

This monitor tracks various Bitcoin transactions. It connects to an Indexer and provides real-time updates on transaction confirmations for different transaction types.


**Transaction Monitoring Types**:
  - **Single Transaction**: Track individual transactions by TXID
  - **Group Transactions**: Monitor multiple transactions as a logical group
  - **RSK Pegin Transactions**: Detect and track RSK pegin transactions
  - **UTXO Spending**: Monitor when specific UTXOs are spent
  
## Features

- **Status Updates**: Get notifications when monitored transactions receive new confirmations
- **Blockchain Synchronization**: Automatically syncs with the Bitcoin blockchain on every tick.

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

The `Monitor` struct provides the following endpoints:

### Core Functionality

- **`is_ready()`**: Checks if the monitor is fully synced with the blockchain.
  - Returns `true` if synced, `false` if still syncing, or an error.

- **`tick()`**: Processes new blocks, updates transaction statuses, and generates news.
  - Should be called periodically to keep the monitor in sync with the blockchain.

- **`add_monitor()`**: Registers a new transaction to be monitored.
  - Supports all transaction types (Single, Group, RSK Pegin, UTXO Spending).

- **`get_transaction_status()`**: Retrieves the current status of a monitored transaction.
  - Includes confirmation count, block information, and transaction details.

### News Management

- **`get_news()`**: Retrieves all pending news items for monitored transactions.
  - News items include confirmation updates and status changes.

- **`acknowledge_news()`**: Marks specific news items as read/processed.
  - Prevents the same news from being returned in subsequent calls.

### Transaction Monitoring

- **`get_monitors()`**: Lists all currently monitored transactions.
  - Includes transaction type and monitoring parameters.

- **`remove_monitor()`**: Stops monitoring a specific transaction.
  - Transaction history remains in the store but no new updates will be generated.

### Blockchain Information

- **`get_confirmation_threshold()`**: Returns the configured confirmation threshold for transactions.
  - Determines how many confirmations are required before a transaction is considered final.

- **`get_monitor_height()`**: Returns the current height the monitor has processed up to.
  - Useful for determining sync status.

