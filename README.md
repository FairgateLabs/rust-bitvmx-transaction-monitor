# BitVMX Transaction Monitor

`bitvmx-transaction-monitor` watches the chain on a consumer's behalf. A consumer subscribes to a fact it cares about, a transaction being mined, an output being spent, a transaction shape appearing in a block, or simply a new block, and the monitor reports that fact as news to be pulled and acknowledged. It is built on [`rust-bitcoin-indexer`](https://github.com/FairgateLabs/rust-bitcoin-indexer), so every answer is counted against the blocks that indexer has actually processed rather than against whatever the node reports at that instant.

## Documentation

The `docs/` folder explains the behaviour a consumer has to reason about, without reading the source:

- [`docs/design.md`](docs/design.md): what each kind of subscription finds, the reporting rules with worked traces, the invariants the monitor keeps, reorg handling, when the node is reached, the storage layout, and a glossary.

## ⚠️ Disclaimer

This library is currently under development and may not be fully stable.
It is not production-ready, has not been audited, and future updates may introduce breaking changes without preserving backward compatibility.

## Key Features

- 🎯 **Four kinds of subscription**: a transaction, the spend of an output, a transaction shape matched by an output pattern, or every new block.
- 🧵 **Independent contexts**: several consumers subscribe to the same target under their own context, and each is answered on its own without hearing the others.
- 🔔 **Confirmation triggers**: a subscription either hears about every block, or hears once at the exact depth it asked for.
- ↩️ **Reorg aware**: what the chain took back is restated against the chain that is left, and a subscription whose transaction lost its block goes back to waiting.
- 📬 **Durable pending news**: what a consumer has not acknowledged is persisted in the order it was decided, so a restart loses nothing and nothing is reported twice.
- 🔌 **Few node calls**: subscribing, cancelling, reading news and acknowledging it touch storage alone, and reading a new block answers every subscription without going back to the node.
- 💾 **Persistent state**: everything lives in `rust-bitvmx-storage-backend`, alongside the indexer's own data, so a restart resumes where it stopped.

### ⚠️ SegWit Requirement For Reliable Tracking

The transaction monitor relies on transaction IDs (txids) to follow confirmations. Legacy (non-SegWit) transactions have malleable txids, so a re-mined transaction could receive a different txid and the monitor would lose track of it—especially because the monitor reports transactions once they reach at least one confirmation and does not enforce SegWit-only inputs. BitVMX currently uses Pay-to-Taproot (P2TR), which is SegWit-based and therefore not susceptible to third-party malleability. If you intend to track legacy transactions, you must either ensure they are SegWit variants (P2WPKH, P2WSH, P2TR, etc.) or reject non-SegWit monitors to avoid missing confirmations.

## System Architecture

| Component | Responsibility |
|---|---|
| `Monitor` | The API. Owns the indexer, the subscriptions and the pending news. |
| `Subscriptions` | Decides what to report, in the three passes described below. |
| `PendingNews` | Holds what the consumer has not acknowledged yet. |
| `MonitorStore` | One database: a record per target, the queue of first checks, the pending news. |
| `helper` | The pure rules: pattern matching, spend detection, confirmation arithmetic. |
| `Indexer` (external) | The chain. Indexes one block per tick, reports reorgs, answers what it holds. |

Each `tick()` advances the indexer by one step and then acts on what that step was. There are three ways a subscription can be answered, and the rest of the documentation calls them by these names:

| Pass | Runs when | What it does |
|---|---|---|
| **Block pass** | the tick indexed a new block | walks that block once, looking for what every subscription is waiting for, and reports whatever is due at this depth |
| **Reorg pass** | the tick unwound a reorg | restates what the removed blocks had been reported for, against the chain that is left |
| **First check** | the first tick after a subscription is registered | the single look into the past that subscription gets, which is the only place the node may be asked about a transaction |

A tick runs the block pass or the reorg pass, never both, because the indexer never unwinds and indexes in the same step. The first check runs on any tick, since it answers what was registered rather than what the chain did. Whatever the passes decide is added to the pending news, and nothing is ever reported outside a `tick()`.

## What You Can Monitor

| Target | Reports | Looks into the past |
|---|---|---|
| `Transaction(txid)` | that transaction, from its first confirmation | yes, and the node when the indexer holds nothing |
| `SpendingUtxo(outpoint)` | the transaction that spends that output | yes, but only the blocks the indexer holds |
| `OutputPattern(filter)` | every transaction whose output at `output_index` is an `OP_RETURN` carrying `tag`, within an optional bound on the number of outputs | no, it matches the blocks that come |
| `NewBlock` | every indexed block, with its height and hash | no |

## Public API

> ⚠️ **A trigger must satisfy `finality < trigger < max_monitoring_confirmations`.** Both bounds are strict, and a trigger outside them is refused with `InvalidConfirmationTrigger`. A trigger is the consumer's finality claim: it is reported once and never restated, which is why it has to sit deeper than a reorg can reach.

> ⚠️ **Acknowledge news only after acting on it.** Acknowledging is the only thing that deletes an item, and it deletes by value. Act first, or lose an event you never processed.

> ⚠️ **Unacknowledged news is never dropped.** A consumer that never acknowledges makes the pending news grow without bound.

> ⚠️ **`cancel` takes that subscription's unacknowledged news with it.** It is the one path that deletes news the consumer has not seen.

> 💡 **News is a snapshot, `get_tx_status` is the current answer.** An item says what was true when it was decided; the query says what the indexer knows now.

> 💡 **`search_in_mempool` only affects a transaction subscription.** It decides whether that subscription's lookup may answer `InMempool`, and whether the txid joins the indexer's mempool watch list. A UTXO or a pattern only ever discovers transactions that are already in a block.

The `Monitor` struct exposes:

| Method | Purpose |
|---|---|
| `new` | Build from an RPC config, a storage handle the consumer owns and optional settings. Validates the settings and reads nothing from the node. |
| `is_ready` | True once the indexer has caught up with the node's tip. |
| `tick` | One step of the chain: advance the indexer, report what that means for every subscription, and answer the ones registered since the last tick. |
| `monitor` | Subscribe one context to a list of targets, with an optional confirmation trigger and the mempool flag. |
| `cancel` | Drop one context from a list of targets, with what it was following and its unacknowledged news. |
| `get_news` | Everything not acknowledged yet. |
| `ack_news` | Acknowledge one item, by the value `get_news` handed over. |
| `get_indexed_height` | Height of the highest block the monitor has processed. |
| `get_block` | The block at a height and hash, from the indexer's storage or from the node. |
| `get_tx_status` | What the indexer knows about a txid right now: confirmed with its block and confirmations, in the mempool, or not found. |
| `rpc_is_utxo_unspent` / `rpc_get_tx_confirmations` | Live node checks, passed straight through. |
| `get_estimated_fee_rate` | Fee rate estimated from the last indexed block. |
| `max_monitoring_confirmations` | The configured maximum. |

## Usage

```rust
use bitvmx_transaction_monitor::{
    config::MonitorConfig,
    monitor::Monitor,
    types::{MonitorTarget, NewsKind, OutputPatternFilter},
};
use std::rc::Rc;
use storage_backend::{storage::Storage, storage_config::StorageConfig};

let config = MonitorConfig::load_config("config/monitor_config.yaml")?;

// The storage belongs to the consumer, and the monitor shares it with the indexer it builds.
let storage = Rc::new(Storage::new(&StorageConfig::new("data".to_string(), None))?);
let monitor = Monitor::new(&config.rpc, storage, Some(config.settings))?;

// Four subscriptions under one context, in one call.
monitor.monitor(
    &[
        MonitorTarget::Transaction(txid),
        MonitorTarget::SpendingUtxo(outpoint),
        MonitorTarget::OutputPattern(OutputPatternFilter {
            output_index: 1,
            tag: b"bitvmx".to_vec(),
            max_outputs: Some(3),
        }),
        MonitorTarget::NewBlock,
    ],
    "my-protocol".to_string(),
    None,  // No trigger, so every block until the maximum.
    false, // No mempool answers for the transaction subscription.
)?;
```

One step of the chain, from the consumer's own loop, and then the news it produced:

```rust
monitor.tick()?;

for item in monitor.get_news()? {
    match &item.kind {
        NewsKind::Transaction { txid, status, due_to_reorg } => {
            info!("{} is {status:?} for {} (reorg: {due_to_reorg})", txid, item.context)
        }
        NewsKind::Block(block) => info!("block {} at {}", block.hash, block.height),
        NewsKind::Unreachable => info!("{:?} can never be answered", item.target),
    }

    // Only after acting on it: this is what deletes it.
    monitor.ack_news(&item)?;
}
```

A subscription ends when the consumer cancels it, or on its own once there is nothing left to report:

```rust
monitor.cancel(&[MonitorTarget::NewBlock], "my-protocol")?;
```

## Configuration

`MonitorConfig` has two sections, `rpc` and `settings`. A sample is in `config/monitor_config.yaml`. The storage is not configured here: the consumer builds it and hands it over, because the indexer and the monitor share one database.

| Setting | Default | Meaning |
|---|---|---|
| `max_monitoring_confirmations` | 100 | How deep a transaction is followed by a subscription with no trigger. It is also the deepest reorg the monitor can still report. |
| `finality` | 6 | The depth at which a block is taken to be settled. Its only job is to be the floor for a trigger. |
| `indexer_settings.retention_depth` | 100 | Forwarded to the indexer: how many recent blocks stay on disk. |

Three relations are validated when the monitor is built, and a configuration that breaks one is refused:

| Relation | Why |
|---|---|
| `max_monitoring_confirmations >= 2` | A transaction has to stay followed for at least one block after its first news, or a reorg that removes it right afterwards is never reported. |
| `max_monitoring_confirmations >= finality + 2` | A trigger is strictly deeper than finality and strictly below the maximum, so there has to be room for one between them. |
| `retention_depth >= max_monitoring_confirmations` | The indexer must still hold the block of anything the monitor is following. |

## Development Setup

Prerequisites:

- Rust
- A Bitcoin node running with `-txindex=1`
- Docker, used by the integration tests

Common commands:

```bash
# Build everything (lib + tests).
cargo build --release --tests

# Run the unit test suite.
cargo test --release --lib

# Run the integration tests (require Docker running; one regtest node, taken one test at a time).
cargo test --release --test regtest
```

The integration tests hold a lock around the node they share, so they need no `--test-threads=1`.

## Contributing 
Contributions are welcome! Please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) file for details.

---

## 🧩 Part of the BitVMX Ecosystem

This repository is a component of the **BitVMX Ecosystem**, an open platform for disputable computation secured by Bitcoin.  
You can find the index of all BitVMX open-source components at [**FairgateLabs/BitVMX**](https://github.com/FairgateLabs/BitVMX).

---
