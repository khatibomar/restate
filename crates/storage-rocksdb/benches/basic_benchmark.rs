// Copyright (c) 2023 -  Restate Software, Inc., Restate GmbH.
// All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

use std::ops::RangeInclusive;

use criterion::{criterion_group, criterion_main, Criterion};
use restate_core::TaskCenterBuilder;
use restate_rocksdb::RocksDbManager;
use restate_storage_api::deduplication_table::{
    DedupSequenceNumber, DeduplicationTable, ProducerId,
};
use restate_storage_api::Transaction;
use restate_storage_rocksdb::{OpenMode, PartitionStore, PartitionStoreManager};
use restate_types::arc_util::Constant;
use restate_types::config::{CommonOptions, WorkerOptions};
use restate_types::identifiers::PartitionKey;
use tokio::runtime::Builder;

async fn writing_to_rocksdb(mut rocksdb: PartitionStore) {
    //
    // write
    //
    for i in 0..100000 {
        let mut txn = rocksdb.transaction();
        for j in 0..10 {
            txn.put_dedup_seq_number(i, ProducerId::Partition(j), DedupSequenceNumber::Sn(0))
                .await;
        }
        txn.commit().await.unwrap();
    }
}

fn basic_writing_reading_benchmark(c: &mut Criterion) {
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();

    let tc = TaskCenterBuilder::default()
        .default_runtime_handle(rt.handle().clone())
        .build()
        .expect("task_center builds");

    let worker_options = WorkerOptions::default();
    tc.run_in_scope_sync("db-manager-init", None, || {
        RocksDbManager::init(Constant::new(CommonOptions::default()))
    });
    let rocksdb = tc.block_on("test-setup", None, async {
        //
        // setup
        //
        let manager = PartitionStoreManager::create(
            Constant::new(worker_options.storage.clone()),
            Constant::new(worker_options.storage.rocksdb.clone()),
            &[(0, RangeInclusive::new(0, PartitionKey::MAX))],
        )
        .await
        .expect("DB creation succeeds");
        manager
            .open_partition_store(
                0,
                RangeInclusive::new(0, PartitionKey::MAX),
                OpenMode::CreateIfMissing,
                &worker_options.storage.rocksdb,
            )
            .await
            .expect("column family is open")
    });

    let mut group = c.benchmark_group("RocksDB");
    group.sample_size(10).bench_function("writing", |bencher| {
        // This will generate a temp dir since we have test-util feature enabled
        bencher
            .to_async(&rt)
            .iter(|| writing_to_rocksdb(rocksdb.clone()));
    });

    group.finish();
    rt.block_on(tc.shutdown_node("completed", 0));
    rt.block_on(RocksDbManager::get().shutdown());
}

criterion_group!(benches, basic_writing_reading_benchmark);
criterion_main!(benches);
