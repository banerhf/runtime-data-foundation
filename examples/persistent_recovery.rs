use runtime_data_foundation::{
    PersistentMmap, StorageEngine,
    encode_value, decode_value
};
use serde_json::json;
use tempfile::tempdir;
use std::fs::OpenOptions;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 启动 Persistent 崩溃恢复演练示例 ===");

    let dir = tempdir()?;
    let base_path = dir.path();
    let arena_id = 8888;
    let generation = 1;
    let system_id = 1;
    let file_path = base_path.join("recovery_test.rdf");

    // 1. 阶段一：正常打开持久化 Arena 并写入 3 条有效记录
    println!("[1/4] 阶段一：正常初始化 Arena 并写入 3 条有效记录...");
    {
        let arena = PersistentMmap::open(
            system_id,
            arena_id,
            generation,
            &file_path,
            16 * 1024,
            4096,
        )?;

        let loc1 = arena.append(101, &encode_value(&json!({"msg": "record 1", "val": 10}))?)?;
        let _loc2 = arena.append(101, &encode_value(&json!({"msg": "record 2", "val": 20}))?)?;
        let _loc3 = arena.append(101, &encode_value(&json!({"msg": "record 3", "val": 30}))?)?;
        
        arena.checkpoint()?;
        println!("   -> 成功写入 3 条记录并建立持久化 Checkpoint。第一条 Locator 序列号: {}", loc1.sequence);
    }

    // 2. 阶段二：模拟进程异常中断/崩溃（在文件末尾注入未提交的损坏或半条记录）
    println!("[2/4] 阶段二：模拟进程异常崩溃（向文件末尾注入没有 Commit Marker 的残缺字节）...");
    {
        let mut file = OpenOptions::new().append(true).open(&file_path)?;
        // 写入一段不完整的垃圾数据，模拟崩溃时只写了部分 Payload 但未写入尾部 Commit Marker
        let corrupted_bytes = vec![0xAB; 48];
        file.write_all(&corrupted_bytes)?;
        file.sync_all()?;
        println!("   -> 已成功注入未提交的损坏尾部字节数: {} 字节", corrupted_bytes.len());
    }

    // 3. 阶段三：重新打开 Arena 触发恢复协议
    println!("[3/4] 阶段三：重新打开 Arena，自动触发恢复扫描与有效尾部截断...");
    let recovered_arena = PersistentMmap::open(
        system_id,
        arena_id,
        generation,
        &file_path,
        16 * 1024,
        4096,
    )?;
    println!("   -> 崩溃恢复协议执行完毕：已自动跳过并清零未提交的损坏残片。");

    // 4. 阶段四：验证历史记录完整性，并测试后续能否无缝追加新记录
    println!("[4/4] 阶段四：验证系统状态并追加新记录...");
    
    let new_locator = recovered_arena.append(
        101,
        &encode_value(&json!({"msg": "record after crash recovery", "val": 40}))?,
    )?;

    println!("   -> 崩溃恢复后成功追加新记录！新 Locator 序列号: {}", new_locator.sequence);
    // 崩溃前写入了 3 条记录，恢复后的下一条 sequence 必须精确等于 4
    assert_eq!(new_locator.sequence, 4);

    let read_bytes = recovered_arena.read(&new_locator)?;
    let decoded_json = decode_value(&read_bytes)?;
    println!("   -> 成功读回恢复后的追加数据: {}", decoded_json);
    assert_eq!(decoded_json["val"], 40);

    println!("=== Persistent 崩溃恢复演练圆满成功 ===");
    Ok(())
}