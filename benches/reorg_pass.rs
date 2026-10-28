//! The reorg pass, which runs on a tick where the indexer unwound blocks. It reads every record and restates every
//! transaction tracked without a trigger.

mod common;

use bitvmx_transaction_monitor::types::MonitorTarget;
use common::{block, plain_tx, Fixture, SIZES};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};

// Setup: N transactions are subscribed without a trigger. Half of them are mined in block 1 and half in block 2, both
// blocks are indexed and passed, and then block 2 is unwound the way the indexer does it: its block is deleted from
// storage and the cursor goes back to 1.
// Measured: `update_after_reorg` and then adding its news to the log, which is what a tick does. All N are asked for in
// one `get_stored_transactions` call. The half in block 1 is found there, with block 1 decoded once for all of them, and
// is restated as confirmed. The half in block 2 is not, so each of those is asked for again on its own, which reads no
// block, and is restated as not found. Their records are written back without the transaction, waiting for it again.
fn reorg_pass(c: &mut Criterion) {
    let mut group = c.benchmark_group("reorg_pass");
    group.sample_size(10);

    for n in SIZES {
        group.throughput(Throughput::Elements(n));
        group.bench_function(BenchmarkId::from_parameter(n), |b| {
            b.iter_batched_ref(
                || {
                    let fixture = Fixture::new();
                    let half = n as u32 / 2;
                    let kept: Vec<_> = (0..half).map(plain_tx).collect();
                    let lost: Vec<_> = (half..n as u32).map(plain_tx).collect();
                    let targets: Vec<_> = kept
                        .iter()
                        .chain(&lost)
                        .map(|tx| MonitorTarget::Transaction(tx.compute_txid()))
                        .collect();
                    fixture
                        .monitor
                        .monitor(&targets, "ctx".to_string(), None, false)
                        .unwrap();
                    fixture.tick_with(&block(1, kept));
                    fixture.tick_with(&block(2, lost));

                    fixture.indexer_store.delete_block(2).unwrap();
                    fixture.indexer_store.save_cursor(1).unwrap();
                    fixture
                },
                |fixture| {
                    let news = fixture.subscriptions.update_after_reorg().unwrap();
                    fixture.news.add(news).unwrap();
                },
                BatchSize::PerIteration,
            )
        });
    }

    group.finish();
}

criterion_group!(benches, reorg_pass);
criterion_main!(benches);
