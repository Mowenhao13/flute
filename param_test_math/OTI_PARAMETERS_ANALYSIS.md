// OTI 参数关系分析文档
// 基于 FLUTE 源码 src/common/oti.rs 的参数约束和计算逻辑

# FLUTE OTI (Object Transmission Information) 参数关系分析

## 概述
本文档基于 FLUTE 项目的源码分析，详细说明了不同 FEC (Forward Error Correction) 方案的参数约束、类型限制和计算逻辑。

## 通用参数类型定义

### 基础参数类型
```rust
// 传输长度限制
const MAX_TRANSFER_LENGTH_48BIT: u64 = 0xFFFF_FFFF_FFFF; // 48 bits (大部分FEC方案)
const MAX_TRANSFER_LENGTH_40BIT: u64 = 0xFF_FFFF_FFFF;   // 40 bits (RaptorQ专用)

// 核心参数类型
pub struct OTIParams {
    pub encoding_symbol_length: u16,        // 符号长度 (bytes)
    pub maximum_source_block_length: u32,   // 最大源块长度 (symbols)  
    pub max_number_of_parity_symbols: u32,  // 最大冗余符号数 (大部分FEC)
    // 或者对于Reed Solomon GF28:
    pub max_number_of_parity_symbols: u8,   // Reed Solomon GF28专用
}
```

## FEC 方案特定约束

### 1. No Code (无编码)
```rust
// 参数类型
encoding_symbol_length: u16
maximum_source_block_length: u32  
max_number_of_parity_symbols: 无 (固定为0)

// 约束条件
- 传输长度限制: 48-bit (0xFFFF_FFFF_FFFF)
- 无冗余符号
- 最简单的传输模式
```

### 2. RaptorQ
```rust
// 参数类型  
encoding_symbol_length: u16
maximum_source_block_length: u32
max_number_of_parity_symbols: u32

// 特殊约束
- 传输长度限制: 40-bit (0xFF_FFFF_FFFF) ⚠️ 比其他方案更严格
- 符号对齐要求: encoding_symbol_length % 4 == 0
- 支持大规模数据传输
- 冗余符号数可变，适合高损耗环境
```

### 3. Raptor (标准)
```rust
// 参数类型
encoding_symbol_length: u16
maximum_source_block_length: u32
max_number_of_parity_symbols: u32

// 约束条件
- 传输长度限制: 48-bit (0xFFFF_FFFF_FFFF)
- 无特殊对齐要求
- 中等复杂度的前向纠错
```

### 4. Reed Solomon GF28
```rust
// 参数类型 (注意冗余符号数的类型差异)
encoding_symbol_length: u16
maximum_source_block_length: u32 (但实际限制 <= 255)
max_number_of_parity_symbols: u8  ⚠️ 类型更小

// 严格约束
- 传输长度限制: 48-bit (0xFFFF_FFFF_FFFF)
- 编码块长度限制: source_symbols + parity_symbols <= 255
- 最大源块数量: u8::MAX (255)
- maximum_source_block_length 实际不能超过 255
- 适合小规模、高可靠性传输
```

### 5. Reed Solomon GF28 Under Specified
```rust
// 参数类型
encoding_symbol_length: u16
maximum_source_block_length: u32
max_number_of_parity_symbols: u8

// 约束条件
- 传输长度限制: 48-bit (0xFFFF_FFFF_FFFF)
- 最大源块数量: u32::MAX (更大的扩展性)
- 编码块长度仍受 255 符号限制
- 比标准GF28支持更多的源块
```

## max_transfer_length 计算逻辑

### 通用计算公式
```rust
fn calculate_max_transfer_length(
    encoding_symbol_length: u16,
    maximum_source_block_length: u32,
    max_source_block_number: u64,
    max_transfer_length_limit: u64
) -> u64 {
    let block_size = encoding_symbol_length as u64 * maximum_source_block_length as u64;
    let theoretical_max = block_size * max_source_block_number;
    std::cmp::min(theoretical_max, max_transfer_length_limit)
}
```

### 各方案具体实现
```rust
// No Code
max_transfer_length = MAX_TRANSFER_LENGTH_48BIT

// RaptorQ  
max_transfer_length = MAX_TRANSFER_LENGTH_40BIT  // ⚠️ 更严格

// Raptor
max_transfer_length = MAX_TRANSFER_LENGTH_48BIT

// Reed Solomon GF28
max_transfer_length = min(
    block_size * u8::MAX,  // 255个块
    MAX_TRANSFER_LENGTH_48BIT
)

// Reed Solomon GF28 Under Specified
max_transfer_length = MAX_TRANSFER_LENGTH_48BIT  // 使用u32::MAX但受48-bit限制
```

## 参数验证检查清单

### RaptorQ 验证
```rust
fn validate_raptorq_params(params: &OTIParams) -> bool {
    params.encoding_symbol_length % 4 == 0 &&                    // 4字节对齐
    params.encoding_symbol_length <= u16::MAX &&                 // u16类型范围
    params.maximum_source_block_length <= u32::MAX &&            // u32类型范围
    params.max_number_of_parity_symbols <= u32::MAX &&           // u32类型范围
    calculate_transfer_length(params) <= MAX_TRANSFER_LENGTH_40BIT // 40-bit限制
}
```

### Reed Solomon GF28 验证
```rust
fn validate_reed_solomon_gf28_params(params: &OTIParams) -> bool {
    params.encoding_symbol_length <= u16::MAX &&                 // u16类型范围
    params.maximum_source_block_length <= 255 &&                 // GF28特殊限制
    params.max_number_of_parity_symbols <= u8::MAX &&            // u8类型范围
    (params.maximum_source_block_length + 
     params.max_number_of_parity_symbols as u32) <= 255 &&       // 总编码块限制
    calculate_transfer_length(params) <= calculate_max_transfer_length_gf28(params)
}
```

## 实际应用建议

### 参数选择指导
1. **小文件 (< 1MB)**: 建议 Reed Solomon GF28，高可靠性
2. **中等文件 (1MB - 100MB)**: 建议 Raptor，平衡性能和可靠性  
3. **大文件 (> 100MB)**: 建议 RaptorQ，但注意40-bit传输长度限制
4. **无损耗环境**: 可以使用 No Code，最高效率

### 常用参数组合
```rust
// 典型局域网配置
encoding_symbol_length: 1400        // 接近MTU大小
maximum_source_block_length: 1024   // 适中的块大小

// 高损耗网络配置  
encoding_symbol_length: 1200        // 保守的包大小
maximum_source_block_length: 512    // 较小的块，便于重传
max_number_of_parity_symbols: 128   // 25%冗余率
```

## 注意事项

### 关键限制
1. **RaptorQ的40-bit限制**: 最大传输 1TB 左右数据
2. **Reed Solomon的255符号限制**: 影响块大小设计
3. **类型范围检查**: 必须在运行时验证参数范围
4. **内存使用**: 大块会显著增加内存消耗

### 性能考虑
1. **符号对齐**: RaptorQ需要4字节对齐以优化性能
2. **块大小平衡**: 大块减少overhead，但增加延迟和内存使用
3. **冗余比例**: 通常10-30%冗余在大多数网络环境下足够

---
*本文档基于 FLUTE 项目 src/common/oti.rs 源码分析生成*
*最后更新: 2024年*