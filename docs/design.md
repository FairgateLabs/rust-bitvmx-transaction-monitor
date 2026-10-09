# Design rules and invariants

The durable rules the monitor is built on, each stated as a fact about the current system with a short reason. They are written in terms of the block pass, the reorg pass and the first check, which the [README](../README.md#system-architecture) defines along with the API and the configuration.

## Operating assumptions

These are the conditions the monitor is designed for. Outside them, behaviour is not guaranteed.

| Assumption | Why it holds |
|---|---|
| A single monitor instance runs against the storage. | One writer per database, shared with the indexer underneath it. |
| The indexer is the only source of chain truth. | Every count comes from the indexer's cursor, never from the node's tip, so an answer never contradicts a block the consumer was already given. |
| `retention_depth >= max_monitoring_confirmations`. | Validated at construction. It is what keeps the block of a tracked transaction inside the indexer's window. |
| A confirmation trigger is at least `finality` deep. | Validated per call. A trigger is reported once and never restated, so what it promised has to be out of reach of a reorg. |
| Txids are stable, because BitVMX transactions are SegWit. | Subscriptions are keyed by txid; a malleable txid would be lost on a re-mine. |
| The consumer acknowledges the news it acts on. | Acknowledging is the only thing that deletes an item. |
| Settings do not change across a restart on the same storage. | Persisted records were written under the maximum and the window that produced them. |

## What each subscription finds

| Target | Identified by | Can a later block still produce an event for it | Ends on its own |
|---|---|---|---|
| `Transaction(txid)` | the txid | no, there is one such transaction and it is already done | yes, with its transaction |
| `SpendingUtxo(outpoint)` | txid and vout | no, an output is spent once | yes, with its spender |
| `OutputPattern(filter)` | every field of the filter | yes, any block may hold a new transaction of that shape | no |
| `NewBlock` | nothing, there is one | yes, every block is one | no |

**A subscription ends when its target can no longer produce anything, not when it has nothing in hand.** A transaction or a UTXO subscription names one event: once the transaction it was waiting for has been tracked to its trigger or to the maximum, no block will ever bring that event again, so the record is deleted and the consumer needs to do nothing. A pattern names a shape and a block subscription names "any block", so for those two an empty list of tracked transactions is the normal state, not a finished one. It is what they look like the moment they are registered, and a pattern that had deleted itself when its last match aged out would silently stop matching the blocks it was created for.

Only `cancel` ends those two, and a consumer that stops caring has to call it: their record is read on every block for as long as it exists.

**A UTXO subscription whose output is older than the indexer's window may never be answered.** `ProbablyUnreachableUTXO` belongs to that kind alone, and this is the only way it is ever reported. Its first check asks the node whether the output is spent, leaving the mempool out, and if it is, looks for the spender in the stored blocks, from the cursor down to the block that created the output and never below it:

| Spent | Spender in the window | Output's block | Answer |
|---|---|---|---|
| no | - | - | wait, the blocks that come bring the spender |
| yes | yes | - | tracked and reported, as any first check |
| yes | no | above the cursor, or not mined | wait, the spender is above the cursor too |
| yes | no | in the window | wait, the spender is above the cursor, every block below it was read |
| yes | no | below the window | `ProbablyUnreachableUTXO`, once per context |

In the last row the spender is either below the window too, or above a cursor that is behind the node, and nothing tells the two apart. So the record is kept: a spender the blocks bring later is reported as any other, and only `cancel` ends it, which is the consumer's job once it decides the spend is old. Registering it again is answered the same way, once. Note that the window is shorter than `retention_depth` while the indexer is still catching up, so a fresh database can look back less far.

**A pattern's identity includes its bound.** `output_index`, `tag` and `max_outputs` are all part of what names the subscription, so the same tag under two different bounds is two subscriptions, each with its own record and its own news. A transaction matching both is reported once for each.

**A pattern and a new block subscription have no past.** Neither looks at a block older than its registration: a pattern matches the blocks that come, and a block subscription reports them.

## Subscriptions and contexts

One record per target holds one entry per context. The record is what the block pass reads; the entry is what decides whether this consumer hears anything. Four things nest, and each answers a different question:

| Level | What it is | Where it lives | How many |
|---|---|---|---|
| Target | what the consumer named | the storage key of a record | one record per target |
| Entry | one consumer's subscription to that target: its context, its trigger, its mempool flag | inside the record | one per context on that target |
| Tracked transaction | something that entry found and is still reporting on: a txid and the block it is in | inside the entry, in `tracked` | none, one, or many, depending on the kind of target |
| News item | one thing to report: the target, the context, and what happened | under the key of the transaction or the height it is about | one per entry per event |

A news item names its target and its context, but it is stored under the txid alone, so one key holds what every subscription has pending about that transaction. Two patterns differing only in their bound are two targets, so a transaction matching both is reported twice, once per target, and both items sit in that one key side by side.

| Rule | Reason |
|---|---|
| Contexts on one target are independent and never hear each other's news. | Each entry carries its own trigger, its own mempool flag and its own list of tracked transactions. |
| Re-registering the same target and context replaces the parameters and keeps what the subscription already found. | Nothing is reported twice, and a reorg is still recognised for what it is already tracking. |
| Re-registering runs the first check again. | A consumer can therefore be told once more about something it already knows. |
| Cancelling removes one entry, and the record when it was the last one. | A record with no entries would be read on every block and answer for nobody. |
| The indexer's mempool watch is dropped only when no remaining entry wants it. | The watch is keyed by txid, so one context leaving must not blind another that is still watching the same transaction. |
| `search_in_mempool` is refused for anything but a transaction target, and a confirmation trigger for a new block subscription. | A UTXO or a pattern discovers transactions that are already in a block, and a block has no confirmations of its own. |
| One call registers every target or none of them. | The refusals above are decided per target, and the records are written after all of them have passed. |

## Invariants

Named guarantees the monitor preserves. They are the reason several paths can be as simple as they are.

### I1: a trigger is reported once and never restated

A subscription with a trigger hears exactly one item about a transaction, at the first block where its count reaches the trigger, and the transaction stops being tracked at that moment. For a transaction or a UTXO target, which end with the transaction they were waiting for, the subscription ends there too.

The consequence is that the reorg pass is silent for every entry that has a trigger. Nothing it still tracks has reached the trigger yet, and anything that did is already gone from the record, so there is no case in which a fired trigger could be taken back. This is the invariant that makes `finality` worth validating: the claim is the consumer's, and the monitor holds it.

A trigger subscriber therefore hears nothing before its trigger, including a reorg that takes the transaction out of its block. If the transaction is never mined again, the subscription waits for ever, as it would for a transaction that was never mined. A consumer that needs to see losses before finality subscribes without a trigger too, or polls `get_tx_status`.

### I2: anything tracked has its block in the indexer

`retention_depth >= max_monitoring_confirmations` means a transaction cannot be tracked past the point where its block would be pruned. The block pass therefore reads transactions from the indexer's storage alone, and an answer other than confirmed is an `InvariantViolation` rather than a reason to ask the node.

### I3: the first check is the only path that may ask the node

It runs at most once per target, from the tick after registration. Everything else a subscription learns comes from the blocks that arrive afterwards. A transaction whose block the indexer read and later pruned is still answered there, from the node, and because a block is only pruned once it is deeper than `retention_depth`, its count is already past the maximum, so it is reported once and never tracked. A transaction mined below the first block the indexer ever read is `UnreachableTx` instead: it happened before the monitor started, and while a fresh database fills its window its count from the cursor could be below the maximum, so tracking it would break I2.

### I4: one tick is one block or one unwind, never both

The indexer answers `Advanced`, `Reorged` or `Idle`, and the monitor runs exactly one pass accordingly. A reorg and a new block are never reported in the same tick, so a count never moves twice in one step.

### I5: confirmations are counted from the indexer's cursor

A transaction in the block at height `h`, with the cursor at `c`, has `c - h + 1` confirmations. The node's tip is never used, so a consumer is never told a depth that rests on a block the monitor has not processed.

### I6: news is append-only per key until acknowledged

Items about one transaction are stored under that transaction, in the order they were decided, and block items under their height. Acknowledging removes one item by value. Nothing else deletes news, except cancelling the subscription it belongs to.

## Reporting rules

| Situation | What the subscription hears |
|---|---|
| No trigger | one item per indexed block, from the block that confirmed the transaction up to and including `max_monitoring_confirmations`, and then nothing |
| Trigger `t` | one item, at the first block where the count reaches `t`. That is the block where it equals `t`, unless a re-registration lowered `t` below the count, and then it is the next block |
| The first check, no trigger | one item, whatever the count |
| The first check, trigger `t` | one item if the count is already at or past `t`, nothing otherwise |
| The lookup finds it at or past the maximum | one item, and nothing is tracked afterwards |
| A reorg | an item for every transaction a subscription without a trigger tracks, marked `due_to_reorg` |
| A transaction subscription whose transaction was mined below the first block the indexer ever read | one `UnreachableTx` item per context, and the record is deleted |
| A UTXO subscription whose output is older than the window, spent with no spender in it | one `ProbablyUnreachableUTXO` item per context, and the record is kept |

The first-check rules use "at or past" where a block uses "exactly". A block arrives one at a time, so a count lands on the trigger; a subscription made long after the fact never would.

### Traces

A transaction with no trigger, mined at 108, maximum 4:

```
Advanced 108   1 confirmation    -> news
Advanced 109   2                 -> news
Advanced 110   3                 -> news
Advanced 111   4                 -> news, the maximum, so it stops being tracked
Advanced 112   -                 -> silent, the subscription is over
```

A trigger of 8 with finality 6, mined at 108. This is I1 end to end, including a reorg deep enough to have broken the promise if the claim had been wrong:

```
Advanced 113   6 confirmations   -> silent
Advanced 114   7                 -> silent
Advanced 115   8                 -> news, and the subscription ends with it
Reorged(2)     back to 6         -> silent, nothing is tracked any more
Advanced 114'  7                 -> silent
Advanced 115'  8                 -> silent, a trigger is never reported twice
```

A UTXO whose spender changes identity. `O` is spent by `txA` in 108, and on the new chain by `txB` in 107', with no trigger:

```
Advanced 108   txA, 1 confirmation                  -> news
Reorged(2)     txA lost its block                   -> news, NotFound, due_to_reorg; txA dropped
Advanced 107'  txB spends O, 1 confirmation         -> news, an ordinary discovery
```

Nothing has to notice that the spender changed. One spender stopped being real and another started, each reported on its own. A transaction re-mined into a different block reads the same way.

## Reorgs

| Event | Handling |
|---|---|
| A tracked transaction keeps its block and only loses depth. | Restated to every subscription without a trigger, with its real status and `due_to_reorg`. |
| A tracked transaction loses its block. | Reported as `InMempool` when that subscription asked for mempool answers, `NotFound` otherwise, then dropped. The subscription goes back to waiting, and the blocks that come may find it again. |
| The output a UTXO subscription watches is spent by a different transaction. | Two independent reports, as in the trace above. |
| A subscription whose trigger already fired. | Silent, by I1. |
| A reorg deeper than `retention_depth`. | The indexer fails with `ReorgDeeperThanWindow` and the monitor stops with it. Out of scope by assumption. |
| A reorg deeper than `max_monitoring_confirmations`. | Not reported for a transaction that already reached the maximum, because it is no longer tracked. |
| A `NewBlock` subscription. | No item of its own. The reorg shows as the replacement blocks the block pass reports afterwards, as below. |

The reorg pass needs no lookup to decide anything: a transaction is gone exactly when its block is above the tip that is left, and the depth of everything else fell by the same amount.

### What a `NewBlock` subscription sees

A reorg of two blocks at the tip `N`, and the order `get_news` hands the items back in:

```
Advanced N       -> Block{N,   H_old}
Reorged          -> nothing, however many ticks the unwind takes
Advanced N-1'    -> Block{N-1, H_new}
Advanced N'      -> Block{N,   H_new}

Acknowledged every tick:   Block{N, H_old}, Block{N-1, H_new}, Block{N, H_new}
All still pending:         Block{N-1, H_new}, Block{N, H_old}, Block{N, H_new}
```

Items about blocks are stored under their height and handed back in height order, not in the order they were decided, so how they come back depends on when the consumer reads. Within one height they are always in the order they were decided. So the reliable sign of a reorg is a height already reported coming back with a different hash, which holds however many ticks pile up. A lower height after a higher one is a sign only for a consumer that acknowledges every tick.

If the chain goes back to the first block, its height holds `[H_old, H_new, H_old]`. Acknowledging removes every copy of the value given, so a consumer that read the first `H_old`, let both reorgs pass, and only then acknowledged it would also remove the second one unseen. Acknowledging in the same loop iteration that read it, before the next tick, avoids that.

## When the node is reached

What the consumer calls:

| Call | Node |
|---|---|
| `monitor`, `cancel`, `get_news`, `ack_news`, `get_indexed_height`, `max_monitoring_confirmations` | never |
| `get_tx_status`, `get_block` | storage first, the node only when the indexer holds nothing |
| `rpc_is_utxo_spent`, `rpc_get_tx_confirmations` | always, which is what the prefix says |
| `is_ready`, `get_estimated_fee_rate` | always, one call each time |

`monitor` is storage alone even with `search_in_mempool` set, because registering a mempool watch is a write in the indexer's storage.

## Storage layout

```
monitor/tx/{txid}                              -> MonitorRecord
monitor/utxo/{txid}:{vout}                     -> MonitorRecord
monitor/pattern/{output_index}:{tag}:{bound}   -> MonitorRecord, bound is the number or "any"
monitor/newblock                               -> MonitorRecord, one for every block subscription

monitor/first_check                            -> the targets waiting for their first check

monitor/news/{txid}                            -> the pending items about one transaction, in order
monitor/news/block/{height:010}                -> the pending items about the blocks at one height
```

A news key is read and written whole, which is what makes the `max_keys` of `get_news` a bound on keys rather than on items: a key is the unit of storage, so it comes back entire.

A record is read and written whole, and found by a prefix scan over its kind, so there is no index and no list that could drift from the records it describes. `{height:010}` is padded to ten digits because keys compare as text, and the padding is what makes block news come back in height order. The tag is hex and every other component is digits, so no component can contain the separator and no two targets can collide.

## Limits

| Limit | Detail |
|---|---|
| A transaction in the mempool is never announced. | Only a block makes the monitor report, and nothing is reported when a transaction is dropped from the mempool either. |
| A spender or a pattern match that falls back into the mempool reads `NotFound`. | Only a transaction subscription creates a mempool watch, and the indexer's snapshot holds only watched txids. |
| The window a spend can be looked for in is shorter while the indexer is catching up. | The monitor never downloads a block to make up for it. |
| Unacknowledged news is unbounded. | See I6: acknowledging is the only thing that deletes an item. |

## Glossary

| Term | Meaning |
|---|---|
| Cursor | The height of the highest block the indexer has processed. Every confirmation count is measured from it. |
| Window | The blocks the indexer still holds, at most `retention_depth` of them. |
| Target | What a consumer asked to watch: a transaction, an output's spend, an output pattern, or every new block. |
| Context | The consumer's own label on a subscription. One target can carry many. |
| Record | The stored value for one target: the target itself and one entry per context. |
| Entry | One context's subscription to that target: its trigger, its mempool flag, and the transactions it tracks. |
| Tracked transaction | A transaction an entry found and is still reporting on, kept as its txid and the block it is in. `TrackedTx` in the code. |
| Trigger | The depth at which a subscription wants its one item. Reported once, never restated. |
| Finality | The depth at which a block is taken to be settled, one block at the minimum. The floor a trigger may sit on. |
| News | One item for one subscription: the target, the context, and what happened. |
| `ProbablyUnreachableUTXO` | The answer to a UTXO subscription whose output is spent and older than anything the indexer holds, with no spender in it. Not final. |
| `UnreachableTx` | The answer to a transaction subscription whose transaction was mined before the first block the indexer ever read. Final. |
