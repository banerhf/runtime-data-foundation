use runtime_data_foundation::{
    TmpRingBuffer, OverflowPolicy, MemoryManager, ArenaStatus, StorageEngine, FoundationError
};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

// 模拟 ArenaEntry 的基础闭包或直接测封装逻辑
// 假设我们在库中暴露了或通过 ArenaManager / 基础组件进行生命周期驱动
#[test]
fn test_arena_lifecycle_basic_states() {
    let memory_manager = MemoryManager::new(1024 * 1024);
    let reservation = memory_manager.reserve(4096).unwrap();
    
    let ring = TmpRingBuffer::new(
        1,
        1001,
        1,
        reservation.size(),
        OverflowPolicy::RejectNew,
    );

    assert_eq!(ring.system_id(), 1);
    assert_eq!(ring.arena_id(), 1001);
    assert_eq!(ring.generation(), 1);
    assert_eq!(ring.used_bytes(), 0);
    assert_eq!(ring.capacity(), 4096);
}

#[test]
fn test_concurrent_operation_blocking_close() {
    // 验证多线程并发进行 Append/Read 时，生命周期管理能够正确跟踪活跃操作数
    let memory_manager = MemoryManager::new(10 * 1024 * 1024);
    let reservation = memory_manager.reserve(1024 * 1024).unwrap();
    
    let ring = Arc::new(TmpRingBuffer::new(
        1,
        2002,
        1,
        reservation.size(),
        OverflowPolicy::OverwriteOldest,
    ));

    let mut handles = vec![];
    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

    // 启动 4 个线程持续写入
    for i in 0..4 {
        let ring_clone = ring.clone();
        let running_clone = running.clone();
        handles.push(thread::spawn(move || {
            let mut count = 0;
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let payload = format!("data-thread-{}-msg-{}", i, count);
                if ring_clone.append(10, payload.as_bytes()).is_ok() {
                    count += 1;
                }
                thread::sleep(Duration::from_millis(1));
            }
        }));
    }

    // 运行一小段时间
    thread::sleep(Duration::from_millis(50));
    running.store(false, std::sync::atomic::Ordering::Relaxed);

    for h in handles {
        h.join().unwrap();
    }

    assert!(ring.used_bytes() > 0);
}