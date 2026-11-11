use runtime_data_foundation::{
    PersistentMmap, StorageEngine, OverflowPolicy, MemoryManager, TmpRingBuffer
};
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

#[test]
fn test_persistent_mmap_multi_threaded_append_read() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("concurrency_arena.rdf");

    let arena = Arc::new(
        PersistentMmap::open(
            1,
            3001,
            1,
            &file_path,
            64 * 1024,
            16 * 1024,
        )
        .unwrap(),
    );

    let thread_count = 4;
    let items_per_thread = 50;
    let mut handles = vec![];

    // 多线程并发 Append
    for t in 0..thread_count {
        let arena_clone = arena.clone();
        handles.push(thread::spawn(move || {
            let mut locators = vec![];
            for i in 0..items_per_thread {
                let msg = format!("t{}-item-{}", t, i);
                let locator = arena_clone.append(100, msg.as_bytes()).unwrap();
                locators.push(locator);
            }
            locators
        }));
    }

    let mut all_locators = vec![];
    for h in handles {
        let locators = h.join().unwrap();
        all_locators.extend(locators);
    }

    // 验证所有 Locator 都可以被多线程安全并发读取并校验通过
    let mut read_handles = vec![];
    for chunk in all_locators.chunks(50) {
        let arena_clone = arena.clone();
        let chunk_locators = chunk.to_vec();
        read_handles.push(thread::spawn(move || {
            for loc in chunk_locators {
                let data = arena_clone.read(&loc).unwrap();
                assert!(!data.is_empty());
            }
        }));
    }

    for h in read_handles {
        h.join().unwrap();
    }
}

#[test]
fn test_tmp_ring_buffer_overflow_and_expiration() {
    let memory_manager = MemoryManager::new(1024 * 1024);
    let reservation = memory_manager.reserve(256).unwrap(); // 很小的容量，触发溢出淘汰

    let ring = Arc::new(TmpRingBuffer::new(
        1,
        4001,
        1,
        reservation.size(),
        OverflowPolicy::OverwriteOldest,
    ));

    // 写入足够多的数据填满并触发覆盖
    let mut locators = vec![];
    for i in 0..20 {
        let payload = vec![i as u8; 30]; // 每条 30 字节
        if let Ok(loc) = ring.append(10, &payload) {
            locators.push((loc, payload));
        }
    }

    // 验证最早写入的记录可能因溢出失效（返回 DataExpired）
    let mut expired_count = 0;
    let mut valid_count = 0;

    for (loc, expected) in locators {
        match ring.read(&loc) {
            Ok(data) => {
                assert_eq!(&*data, &expected);
                valid_count += 1;
            }
            Err(runtime_data_foundation::FoundationError::DataExpired) => {
                expired_count += 1;
            }
            _ => panic!("unexpected error type"),
        }
    }

    assert!(expired_count > 0, "Expected some old records to be overwritten and expired");
    assert!(valid_count > 0, "Expected remaining records to be valid");
}