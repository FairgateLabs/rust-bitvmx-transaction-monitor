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

### Run Tests
```rust
$ cargo test
```