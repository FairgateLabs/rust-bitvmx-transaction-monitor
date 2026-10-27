//! The block pass, which runs on every block the indexer reads. Each group measures `process_block` on one block of
//! `BLOCK_TXS` transactions, with N subscriptions in a different state.

mod common;

use bitvmx_transaction_monitor::types::MonitorTarget;
use common::{block, plain_tx, tagged_tx, unanswered_targets, Fixture, SIZES};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};

// Setup: N transactions, N outputs and N patterns are subscribed, and block 1 is indexed with none of them in it.
// Measured: `process_block` on block 1. It reads every record and checks every transaction of the block against them,
// finds nothing, and writes nothing. This is the cost of a block while every subscription is still waiting.
fn block_pass_idle(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_pass_idle");
    group.sample_size(10);

    for n in SIZES {
        let fixture = Fixture::new();
        fixture
            .monitor
            .monitor(&unanswered_targets(n), "ctx".to_string(), None, false)
            .unwrap();
        let next = block(1, vec![]);
        fixture.index_block(&next);

        group.throughput(Throughput::Elements(n));
        // The pass writes nothing here, so the same database is reused by every iteration with `iter`.
        group.bench_with_input(BenchmarkId::from_parameter(n), &next, |b, next| {
            b.iter(|| fixture.subscriptions.process_block(next).unwrap())
        });
    }

    group.finish();
}

// Setup: N transactions are subscribed without a trigger, block 1 holding all of them is indexed and passed, so every
// subscription is tracking its transaction, and block 2 is indexed with none of them.
// Measured: `process_block` on block 2. Each of the N transactions has 2 confirmations and is reported, and each report
// reads block 1 from storage and decodes it whole to get the transaction (the TODO: in advance_to_block).
fn block_pass_tracking(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_pass_tracking");
    group.sample_size(10);

    for n in SIZES {
        let fixture = Fixture::new();
        let mined: Vec<_> = (0..n as u32).map(plain_tx).collect();
        let targets: Vec<_> = mined
            .iter()
            .map(|tx| MonitorTarget::Transaction(tx.compute_txid()))
            .collect();
        fixture
            .monitor
            .monitor(&targets, "ctx".to_string(), None, false)
            .unwrap();
        fixture.tick_with(&block(1, mined));

        let next = block(2, vec![]);
        fixture.index_block(&next);

        group.throughput(Throughput::Elements(n));
        // Two confirmations is below the maximum, so no transaction stops being tracked and the pass writes nothing.
        // The same database is reused by every iteration with `iter`.
        group.bench_with_input(BenchmarkId::from_parameter(n), &next, |b, next| {
            b.iter(|| fixture.subscriptions.process_block(next).unwrap())
        });
    }

    group.finish();
}

// Setup: one pattern is subscribed under contexts "a", "b" and "c", and block 1 is indexed with N transactions that match it.
// Measured: `process_block` on block 1 and then adding its news to the log, which is what a tick does. The pass starts
// tracking the N matches in the pattern's record and writes it, and reports each match once per context, so 3N reports,
// each of which decodes block 1. Adding the 3N items groups them into N keys, one per transaction.
fn block_pass_pattern_matches(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_pass_pattern_matches");
    group.sample_size(10);

    for n in SIZES {
        let matching = block(1, (0..n as u32).map(tagged_tx).collect());

        group.throughput(Throughput::Elements(n));
        group.bench_with_input(BenchmarkId::from_parameter(n), &matching, |b, matching| {
            b.iter_batched_ref(
                || {
                    let fixture = Fixture::new();
                    let pattern = [MonitorTarget::OutputPattern(common::pattern(0))];
                    for context in ["a", "b", "c"] {
                        fixture
                            .monitor
                            .monitor(&pattern, context.to_string(), None, false)
                            .unwrap();
                    }
                    fixture.index_block(matching);
                    fixture
                },
                |fixture| {
                    let news = fixture.subscriptions.process_block(matching).unwrap();
                    fixture.news.add(news).unwrap();
                },
                BatchSize::PerIteration,
            )
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    block_pass_idle,
    block_pass_tracking,
    block_pass_pattern_matches
);
criterion_main!(benches);
