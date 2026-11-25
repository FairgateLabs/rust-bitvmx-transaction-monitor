//! The calls a consumer makes outside a tick: subscribing, cancelling, and reading and acknowledging news. Each one is
//! called through `Monitor`, the same way a consumer calls it.

mod common;
use bitvmx_transaction_monitor::test_utils::news;
use bitvmx_transaction_monitor::types::{MonitorNews, MonitorTarget, NewsKind};
use bitvmx_transaction_monitor::TransactionStatus;
use common::{fake_txid, unanswered_targets, Fixture, SIZES};
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};

// Registers N transactions, N outputs and N patterns under one context, into an empty database, in two ways:
// - one_call: a single `monitor` call with all 3N targets.
// - one_per_call: 3N `monitor` calls with one target each.
fn subscribe(c: &mut Criterion) {
    let mut group = c.benchmark_group("subscribe");
    group.sample_size(10);

    for n in SIZES {
        let targets = unanswered_targets(n);
        group.throughput(Throughput::Elements(n));

        group.bench_with_input(BenchmarkId::new("one_call", n), &targets, |b, targets| {
            b.iter_batched_ref(
                Fixture::new,
                |fixture| {
                    fixture
                        .monitor
                        .monitor(targets, "ctx".to_string(), None, false)
                        .unwrap()
                },
                BatchSize::PerIteration,
            )
        });

        group.bench_with_input(
            BenchmarkId::new("one_per_call", n),
            &targets,
            |b, targets| {
                b.iter_batched_ref(
                    Fixture::new,
                    |fixture| {
                        for target in targets {
                            fixture
                                .monitor
                                .monitor(
                                    std::slice::from_ref(target),
                                    "ctx".to_string(),
                                    None,
                                    false,
                                )
                                .unwrap();
                        }
                    },
                    BatchSize::PerIteration,
                )
            },
        );
    }

    group.finish();
}

// Contexts "ctx" and "other" both subscribe to the same N transactions with the mempool watch on, and to new
// blocks, and both have one item of news pending about each transaction.
// One `cancel` call for "ctx" on the N transactions and the new block target. It deletes the "ctx" entry
// from every record, keeps the mempool watches because "other" still wants them, and drops the N news items of "ctx"
// while keeping those of "other". The new block target is in the list so the cancel takes the path that reads the
// whole news log, once for all the targets.
fn cancel(c: &mut Criterion) {
    let mut group = c.benchmark_group("cancel");
    group.sample_size(10);

    for n in SIZES {
        let transactions: Vec<_> = (0..n as u32).map(|i| fake_txid(1, i)).collect();
        let targets: Vec<_> = transactions
            .iter()
            .map(|txid| MonitorTarget::Transaction(*txid))
            .collect();
        let cancelled: Vec<_> = targets
            .iter()
            .cloned()
            .chain([MonitorTarget::NewBlock])
            .collect();

        group.throughput(Throughput::Elements(n));
        group.bench_function(BenchmarkId::from_parameter(n), |b| {
            b.iter_batched_ref(
                || {
                    let fixture = Fixture::new();
                    for context in ["ctx", "other"] {
                        fixture
                            .monitor
                            .monitor(&targets, context.to_string(), None, true)
                            .unwrap();
                        fixture
                            .monitor
                            .monitor(&[MonitorTarget::NewBlock], context.to_string(), None, false)
                            .unwrap();
                        fixture
                            .news
                            .add(
                                transactions
                                    .iter()
                                    .map(|txid| not_found(*txid, context))
                                    .collect(),
                            )
                            .unwrap();
                    }
                    fixture
                },
                |fixture| fixture.monitor.cancel(&cancelled, "ctx").unwrap(),
                BatchSize::PerIteration,
            )
        });
    }

    group.finish();
}

// The news log with N transactions and one item about each for contexts "a" and "b", so 2N items under N keys:
// - add: stores the 2N items into an empty log in one call, all in one tick, which groups them by transaction and
//   writes each key once.
// - get_and_ack: with the 2N items already stored one transaction per tick, reads them all with one `get_news` and then
//   acknowledges them one at a time.
// - drain_max_keys_K: with the 2N items already stored one transaction per tick, calls `get_news(Some(K))` and
//   acknowledges what it returns, again and again until it returns nothing, which is how a consumer reads a large
//   backlog with a limit.
// - drain_one_tick_max_keys_K: the same drain, with the 2N items stored in one single tick, as an attack that fills
//   one block would leave them. Every call reads that whole tick again to order it.
fn news_cycle(c: &mut Criterion) {
    const LIMITS: [usize; 3] = [10, 50, 100]; // The `max_keys` values drain_max_keys is run with.

    let mut group = c.benchmark_group("news_cycle");
    group.sample_size(10);

    // One size more than the other groups, the size of a backlog left by an attack.
    for n in SIZES.into_iter().chain([10_000]) {
        let items: Vec<_> = (0..n as u32)
            .flat_map(|i| ["a", "b"].map(|context| not_found(fake_txid(1, i), context)))
            .collect();
        group.throughput(Throughput::Elements(n));

        group.bench_with_input(BenchmarkId::new("add", n), &items, |b, items| {
            b.iter_batched_ref(
                // The setup hands over its own copy of the items, so the measured closure can move them into `add`.
                || (Fixture::new(), items.clone()),
                |(fixture, items)| fixture.news.add(std::mem::take(items)).unwrap(),
                BatchSize::PerIteration,
            )
        });

        group.bench_with_input(BenchmarkId::new("get_and_ack", n), &items, |b, items| {
            b.iter_batched_ref(
                || backlog(items),
                |fixture| {
                    for item in fixture.monitor.get_news(None).unwrap() {
                        fixture.monitor.ack_news(&item).unwrap();
                    }
                },
                BatchSize::PerIteration,
            )
        });

        for limit in LIMITS {
            let id = BenchmarkId::new(format!("drain_max_keys_{limit}"), n);
            group.bench_with_input(id, &items, |b, items| {
                b.iter_batched_ref(
                    || backlog(items),
                    |fixture| drain(fixture, limit),
                    BatchSize::PerIteration,
                )
            });

            let id = BenchmarkId::new(format!("drain_one_tick_max_keys_{limit}"), n);
            group.bench_with_input(id, &items, |b, items| {
                b.iter_batched_ref(
                    || {
                        let fixture = Fixture::new();
                        fixture.news.next_tick().unwrap();
                        fixture.news.add(items.clone()).unwrap();
                        fixture
                    },
                    |fixture| drain(fixture, limit),
                    BatchSize::PerIteration,
                )
            });
        }
    }

    group.finish();
}

/// One item of news saying this transaction is not found, as the reorg pass reports one whose block was unwound.
fn not_found(txid: bitcoin::Txid, context: &str) -> MonitorNews {
    news(
        MonitorTarget::Transaction(txid),
        context,
        NewsKind::Transaction {
            txid,
            status: TransactionStatus::NotFound,
            due_to_reorg: true,
        },
    )
}

/// A monitor whose news log holds these items one transaction per tick, which is how a backlog builds up over many
/// blocks. The items of one transaction are next to each other, two per transaction.
fn backlog(items: &[MonitorNews]) -> Fixture {
    let fixture = Fixture::new();
    for tick in items.chunks(2) {
        fixture.news.next_tick().unwrap();
        fixture.news.add(tick.to_vec()).unwrap();
    }
    fixture
}

/// Calls `get_news(Some(limit))` and acknowledges what it returns until it returns nothing.
fn drain(fixture: &Fixture, limit: usize) {
    loop {
        let news = fixture.monitor.get_news(Some(limit)).unwrap();
        if news.is_empty() {
            break;
        }
        for item in &news {
            fixture.monitor.ack_news(item).unwrap();
        }
    }
}

criterion_group!(benches, subscribe, cancel, news_cycle);
criterion_main!(benches);
