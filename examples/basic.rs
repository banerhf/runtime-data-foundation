use runtime_data_foundation::{
    SchemaRegistry, Schema, FieldSchema, FieldType,
    MemoryManager, TmpRingBuffer, StorageEngine, OverflowPolicy,
    encode_value, decode_value
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 启动 Runtime Data Foundation 基础示例 ===");

    // 1. 初始化 Schema 注册表并注册用户事件 Schema
    let registry = SchemaRegistry::default();
    let user_schema = Schema {
        type_id: 1001,
        name: "UserEvent".into(),
        version: 1,
        fields: vec![
            FieldSchema {
                name: "user_id".into(),
                field_type: FieldType::U64,
                nullable: false,
                required: true,
            },
            FieldSchema {
                name: "action".into(),
                field_type: FieldType::String,
                nullable: false,
                required: true,
            },
            FieldSchema {
                name: "score".into(),
                field_type: FieldType::F64,
                nullable: true,
                required: false,
            },
        ],
    };

    registry.register(user_schema.clone())?;
    println!("[1/5] Schema 注册成功: type_id = {}", user_schema.type_id);

    // 2. 初始化全局内存管理器，并为 Arena 申请 RAII 内存预算 (64 KB)
    let memory_manager = MemoryManager::new(1024 * 1024); // 1 MB 总配额
    let reservation = memory_manager.reserve(64 * 1024)?;
    println!("[2/5] 内存预算分配成功: 已分配 {} 字节", reservation.size());

    // 3. 创建基于临时环形缓冲区的存储引擎 (TMP RingBuffer)
    let storage = TmpRingBuffer::new(
        1,    // system_id
        5001, // arena_id
        1,    // generation
        reservation.size(),
        OverflowPolicy::RejectNew,
    );
    println!("[3/5] 存储引擎初始化完成 (Arena ID: {})", storage.arena_id());

    // 4. 构造待写入数据，并通过 Schema 进行运行时强校验
    let event_payload = json!({
        "user_id": 987654321_u64,
        "action": "execute_query",
        "score": 99.45
    });

    user_schema.validate(&event_payload)?;
    println!("[4/5] 数据结构校验通过: {}", event_payload);

    // 5. 编码并追加数据到 Arena，获取不可变 DataLocator 定位符
    let encoded_data = encode_value(&event_payload)?;
    let locator = storage.append(user_schema.type_id, &encoded_data)?;
    println!("[5/5] 数据写入成功！生成 Locator: {:?}", locator);

    // 6. 根据 Locator 读取并解码数据
    let raw_bytes = storage.read(&locator)?;
    let decoded_json = decode_value(&raw_bytes)?;
    println!("--------------------------------------------------");
    println!("-> 成功从存储引擎读回并解码数据: {}", decoded_json);
    println!("--------------------------------------------------");

    assert_eq!(decoded_json["user_id"], 987654321);
    assert_eq!(decoded_json["action"], "execute_query");

    println!("=== 基础运行示例顺利完成 ===");
    Ok(())
}