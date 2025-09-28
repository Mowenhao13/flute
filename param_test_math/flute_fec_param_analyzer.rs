// flute_fec_param_analyzer.rs
// FLUTE FEC 参数分析器 - 支持所有FEC方案的完整参数验证

// 常量定义
const KB: u64 = 1024;
const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * 1024 * 1024;

// FEC方案传输长度限制
const MAX_TRANSFER_LENGTH_48BIT: u64 = 0xFFFF_FFFF_FFFF; // 48 bits (NoCode, Raptor, Reed Solomon)
const MAX_TRANSFER_LENGTH_40BIT: u64 = 0xFF_FFFF_FFFF;   // 40 bits (RaptorQ)

// UDP数据包限制
const UDP_MAX_PAYLOAD: u16 = 65507;  // UDP最大有效载荷 = 65535 - 8(UDP头) - 20(IP头)
const ETHERNET_MTU_PAYLOAD: u16 = 1472;  // 标准以太网MTU下UDP最大载荷 = 1500 - 8 - 20

// 简单的数学函数
fn div_ceil(dividend: u64, divisor: u64) -> u64 {
    (dividend + divisor - 1) / divisor
}

fn div_floor(dividend: u64, divisor: u64) -> u64 {
    dividend / divisor
}

// FEC 方案枚举
#[derive(Clone, Copy, Debug)]
enum FecScheme {
    NoCode,
    RaptorQ,
    Raptor, 
    ReedSolomonGF28,
    ReedSolomonGF28UnderSpecified,
}

// 通用参数结构
#[derive(Clone, Debug)]
struct FecParams {
    scheme: FecScheme,
    encoding_symbol_length: u16,
    maximum_source_block_length: u32,  // 注意：内部可能转换为不同类型
    max_number_of_parity_symbols: u32,  // 注意：内部可能转换为不同类型
    sub_blocks_length: Option<u16>,     // RaptorQ专用
    symbol_alignment: Option<u8>,       // RaptorQ专用
}

fn main() {
    println!("🔬 FLUTE FEC 参数分析器 v2.0");
    println!("支持所有FEC方案的完整参数验证");
    println!("{}", "=".repeat(60));
    
    let test_file_size = 1024 * MB; // 1GB测试文件
    
    // ===== RaptorQ 测试配置 =====
    println!("\n🟡 RaptorQ FEC 测试");
    
    // 实际FLUTE配置
    test_fec_configuration(FecParams {
        scheme: FecScheme::RaptorQ,
        encoding_symbol_length: 38912,  // 38KB (实际配置)
        maximum_source_block_length: 120,
        max_number_of_parity_symbols: 100,
        sub_blocks_length: Some(1),
        symbol_alignment: Some(8),
    }, test_file_size, "RaptorQ实际配置");
    
    // 2的幂优化配置
    test_fec_configuration(FecParams {
        scheme: FecScheme::RaptorQ,
        encoding_symbol_length: 32768,  // 32KB = 2^15
        maximum_source_block_length: 256,
        max_number_of_parity_symbols: 64,
        sub_blocks_length: Some(1),
        symbol_alignment: Some(8),
    }, test_file_size, "RaptorQ优化配置");
    
    // 高效率配置（符合UDP限制） 
    test_fec_configuration(FecParams {
        scheme: FecScheme::RaptorQ,
        encoding_symbol_length: 8192,   // 8KB，符合UDP限制且8字节对齐
        maximum_source_block_length: 1024,
        max_number_of_parity_symbols: 64,
        sub_blocks_length: Some(1),
        symbol_alignment: Some(8),
    }, test_file_size, "RaptorQ高效配置(UDP兼容)");
    
    // UDP边界测试配置（8字节对齐）
    test_fec_configuration(FecParams {
        scheme: FecScheme::RaptorQ,
        encoding_symbol_length: 65504,  // UDP最大载荷，8字节对齐 (65504 % 8 = 0)
        maximum_source_block_length: 250,
        max_number_of_parity_symbols: 20,
        sub_blocks_length: Some(1),
        symbol_alignment: Some(8),
    }, test_file_size, "RaptorQ UDP边界测试(8字节对齐)");
    
    // 以太网MTU兼容配置
    test_fec_configuration(FecParams {
        scheme: FecScheme::RaptorQ,
        encoding_symbol_length: ETHERNET_MTU_PAYLOAD,  // 以太网MTU载荷
        maximum_source_block_length: 500,
        max_number_of_parity_symbols: 30,
        sub_blocks_length: Some(1),
        symbol_alignment: Some(8),
    }, test_file_size, "RaptorQ 以太网兼容配置");
    
    // ===== Reed Solomon GF28 测试配置 =====
    println!("\n🔴 Reed Solomon GF28 测试");
    
    // 小文件适配配置
    test_fec_configuration(FecParams {
        scheme: FecScheme::ReedSolomonGF28,
        encoding_symbol_length: 1400,
        maximum_source_block_length: 200,
        max_number_of_parity_symbols: 50,
        sub_blocks_length: None,
        symbol_alignment: None,
    }, 10 * MB, "GF28小文件配置");
    
    // 大符号配置
    test_fec_configuration(FecParams {
        scheme: FecScheme::ReedSolomonGF28,
        encoding_symbol_length: 8192,   // 8KB符号
        maximum_source_block_length: 200,
        max_number_of_parity_symbols: 50,
        sub_blocks_length: None,
        symbol_alignment: None,
    }, 100 * MB, "GF28大符号配置");
    
    // ===== Reed Solomon GF28 UnderSpecified 测试 =====
    println!("\n🔵 Reed Solomon GF28 UnderSpecified 测试");
    
    test_fec_configuration(FecParams {
        scheme: FecScheme::ReedSolomonGF28UnderSpecified,
        encoding_symbol_length: 1400,
        maximum_source_block_length: 200,
        max_number_of_parity_symbols: 50,
        sub_blocks_length: None,
        symbol_alignment: None,
    }, test_file_size, "GF28 UnderSpecified配置");
    
    // ===== Raptor 测试配置 =====
    println!("\n🟠 Raptor FEC 测试");
    
    test_fec_configuration(FecParams {
        scheme: FecScheme::Raptor,
        encoding_symbol_length: 1400,
        maximum_source_block_length: 1024,
        max_number_of_parity_symbols: 256,
        sub_blocks_length: None,
        symbol_alignment: None,
    }, test_file_size, "Raptor标准配置");
    
    // ===== NoCode 测试配置 =====
    println!("\n⚪ No Code 测试");
    
    test_fec_configuration(FecParams {
        scheme: FecScheme::NoCode,
        encoding_symbol_length: 1400,
        maximum_source_block_length: 1024,
        max_number_of_parity_symbols: 0,  // NoCode无冗余
        sub_blocks_length: None,
        symbol_alignment: None,
    }, test_file_size, "NoCode配置");
    
    // ===== 类型范围验证 =====
    println!("\n📏 类型范围验证测试");
    test_type_limits();
}

fn test_fec_configuration(params: FecParams, transfer_length: u64, description: &str) {
    println!("\n--- {} ---", description);
    println!("FEC方案: {:?}", params.scheme);
    println!("参数:");
    println!("  - 文件大小: {} MB", transfer_length / MB);
    println!("  - 符号长度: {} bytes ({} KB)", 
        params.encoding_symbol_length, params.encoding_symbol_length / 1024);
    println!("  - 最大源块长度: {} symbols", params.maximum_source_block_length);
    println!("  - 最大冗余符号: {} symbols", params.max_number_of_parity_symbols);
    
    if let Some(alignment) = params.symbol_alignment {
        println!("  - 符号对齐: {} bytes", alignment);
    }
    if let Some(sub_blocks) = params.sub_blocks_length {
        println!("  - 子块长度: {}", sub_blocks);
    }
    
    // ===== 基础计算 =====
    let symbol_count = div_ceil(transfer_length, params.encoding_symbol_length as u64);
    let nb_block = div_ceil(symbol_count, params.maximum_source_block_length as u64);
    let a_large = div_ceil(symbol_count, nb_block);
    let a_small = div_floor(symbol_count, nb_block);
    let nb_a_large = symbol_count - (a_small * nb_block);
    
    // 编码后参数
    let max_encoded_symbols = symbol_count + (nb_block * params.max_number_of_parity_symbols as u64);
    let transfer_length_after = max_encoded_symbols * params.encoding_symbol_length as u64;
    
    println!("计算结果:");
    println!("  - 总符号数: {}", symbol_count);
    println!("  - 总块数: {}", nb_block);
    println!("  - 大块: {} symbols ({} 个), 小块: {} symbols", a_large, nb_a_large, a_small);
    println!("  - 编码后大小: {} MB", transfer_length_after / MB);
    
    if params.max_number_of_parity_symbols > 0 {
        let overhead_percentage = (transfer_length_after as f64 - transfer_length as f64) / transfer_length as f64 * 100.0;
        println!("  - 传输开销: {:.2}%", overhead_percentage);
    } else {
        println!("  - 传输开销: 0% (无编码)");
    }
    
    // ===== 方案特定验证 =====
    let validation_result = validate_fec_params(&params, transfer_length_after, nb_block, 
                                              a_large.max(a_small));
    
    println!("验证结果:");
    for (check, result) in &validation_result {
        println!("  {} {}: {}", 
            if *result { "✓" } else { "✗" }, 
            check, 
            if *result { "通过" } else { "失败" });
    }
    
    // 总体评估
    let all_valid = validation_result.iter().all(|(_, result)| *result);
    println!("=> 总体评估: {}", if all_valid { "✅ 通过" } else { "❌ 失败" });
    
    if all_valid && params.max_number_of_parity_symbols > 0 {
        let overhead_percentage = (transfer_length_after as f64 - transfer_length as f64) / transfer_length as f64 * 100.0;
        if overhead_percentage < 10.0 {
            println!("   🚀 低开销，高效率配置");
        } else if overhead_percentage < 30.0 {
            println!("   ⚡ 中等开销，平衡配置");
        } else {
            println!("   🛡️ 高开销，高可靠性配置");
        }
    }
}

fn validate_fec_params(params: &FecParams, transfer_length_after: u64, nb_block: u64, _max_block_length: u64) -> Vec<(&'static str, bool)> {
    let mut results = Vec::new();
    
    match params.scheme {
        FecScheme::RaptorQ => {
            // RaptorQ特殊检查
            
            // 1. 符号对齐检查
            if let Some(alignment) = params.symbol_alignment {
                let alignment_valid = (params.encoding_symbol_length % alignment as u16) == 0;
                results.push(("符号对齐", alignment_valid));
            }
            
            // 2. 40-bit传输长度限制
            let transfer_length_valid = transfer_length_after <= MAX_TRANSFER_LENGTH_40BIT;
            results.push(("传输长度(40-bit)", transfer_length_valid));
            
            // 3. u16类型范围检查
            results.push(("符号长度范围(u16)", params.encoding_symbol_length <= u16::MAX));
            results.push(("源块长度范围(u16)", params.maximum_source_block_length <= u16::MAX as u32));
            results.push(("冗余符号范围(u16)", params.max_number_of_parity_symbols <= u16::MAX as u32));
        },
        
        FecScheme::ReedSolomonGF28 => {
            // Reed Solomon GF28特殊限制
            
            // 1. 编码块长度限制 (source + parity <= 255)
            let encoding_block_length = params.maximum_source_block_length + params.max_number_of_parity_symbols;
            let encoding_block_valid = encoding_block_length <= 255;
            results.push(("编码块长度(≤255)", encoding_block_valid));
            
            // 2. 源块长度限制
            let source_block_valid = params.maximum_source_block_length <= 255;
            results.push(("源块长度(≤255)", source_block_valid));
            
            // 3. 冗余符号类型限制 (u8)
            let parity_type_valid = params.max_number_of_parity_symbols <= u8::MAX as u32;
            results.push(("冗余符号范围(u8)", parity_type_valid));
            
            // 4. 总块数限制 (GF28: u8::MAX)
            let nb_block_valid = nb_block <= u8::MAX as u64;
            results.push(("总块数(≤255)", nb_block_valid));
            
            // 5. 48-bit传输长度限制
            let transfer_length_valid = transfer_length_after <= MAX_TRANSFER_LENGTH_48BIT;
            results.push(("传输长度(48-bit)", transfer_length_valid));
        },
        
        FecScheme::ReedSolomonGF28UnderSpecified => {
            // UnderSpecified变体：放宽块数限制，但保持编码块限制
            
            // 1. 编码块长度限制仍然存在
            let encoding_block_length = params.maximum_source_block_length + params.max_number_of_parity_symbols;
            let encoding_block_valid = encoding_block_length <= 255;
            results.push(("编码块长度(≤255)", encoding_block_valid));
            
            // 2. 冗余符号类型限制 (u8)
            let parity_type_valid = params.max_number_of_parity_symbols <= u8::MAX as u32;
            results.push(("冗余符号范围(u8)", parity_type_valid));
            
            // 3. 总块数限制 (UnderSpecified: u32::MAX)
            let nb_block_valid = nb_block <= u32::MAX as u64;
            results.push(("总块数(u32)", nb_block_valid));
            
            // 4. 48-bit传输长度限制
            let transfer_length_valid = transfer_length_after <= MAX_TRANSFER_LENGTH_48BIT;
            results.push(("传输长度(48-bit)", transfer_length_valid));
        },
        
        FecScheme::Raptor => {
            // Raptor标准检查
            
            // 1. 48-bit传输长度限制
            let transfer_length_valid = transfer_length_after <= MAX_TRANSFER_LENGTH_48BIT;
            results.push(("传输长度(48-bit)", transfer_length_valid));
            
            // 2. 类型范围检查 (假设使用u32)
            results.push(("符号长度范围(u16)", params.encoding_symbol_length <= u16::MAX));
            results.push(("源块长度范围(u32)", params.maximum_source_block_length <= u32::MAX));
            results.push(("冗余符号范围(u32)", params.max_number_of_parity_symbols <= u32::MAX));
        },
        
        FecScheme::NoCode => {
            // NoCode最简单检查
            
            // 1. 48-bit传输长度限制
            let transfer_length_valid = transfer_length_after <= MAX_TRANSFER_LENGTH_48BIT;
            results.push(("传输长度(48-bit)", transfer_length_valid));
            
            // 2. 无冗余符号
            let no_parity_valid = params.max_number_of_parity_symbols == 0;
            results.push(("无冗余符号", no_parity_valid));
            
            // 3. 类型范围检查
            results.push(("符号长度范围(u16)", params.encoding_symbol_length <= u16::MAX));
            results.push(("源块长度范围(u32)", params.maximum_source_block_length <= u32::MAX));
        },
    }
    
    // 通用检查
    results.push(("符号长度非零", params.encoding_symbol_length > 0));
    results.push(("源块长度非零", params.maximum_source_block_length > 0));
    
    // UDP数据包大小限制检查
    let udp_payload_valid = params.encoding_symbol_length <= UDP_MAX_PAYLOAD;
    results.push(("UDP载荷限制(≤65507)", udp_payload_valid));
    
    let ethernet_mtu_valid = params.encoding_symbol_length <= ETHERNET_MTU_PAYLOAD;
    results.push(("以太网MTU兼容(≤1472)", ethernet_mtu_valid));
    
    results
}

fn test_type_limits() {
    println!("--- 类型限制验证 ---");
    
    println!("RaptorQ 参数类型范围:");
    println!("  encoding_symbol_length: u16 (0 - {})", u16::MAX);
    println!("  maximum_source_block_length: u16 (0 - {})", u16::MAX);  
    println!("  max_number_of_parity_symbols: u16 (0 - {})", u16::MAX);
    println!("  sub_blocks_length: u16 (0 - {})", u16::MAX);
    println!("  symbol_alignment: u8 (0 - {})", u8::MAX);
    
    println!("\nReed Solomon GF28 参数类型范围:");
    println!("  encoding_symbol_length: u16 (0 - {})", u16::MAX);
    println!("  maximum_source_block_length: u32 (实际限制 ≤ 255)");
    println!("  max_number_of_parity_symbols: u8 (0 - {})", u8::MAX);
    println!("  编码块总长度: source + parity ≤ 255");
    
    println!("\n传输长度限制:");
    println!("  RaptorQ: 40-bit ({} TB)", MAX_TRANSFER_LENGTH_40BIT / GB / 1024);
    println!("  其他FEC: 48-bit ({} TB)", MAX_TRANSFER_LENGTH_48BIT / GB / 1024);
    
    println!("\nUDP数据包大小限制:");
    println!("  UDP理论最大载荷: {} bytes", UDP_MAX_PAYLOAD);
    println!("  标准以太网MTU载荷: {} bytes", ETHERNET_MTU_PAYLOAD);
    println!("  ⚠️  encoding_symbol_length不得超过UDP最大有效载荷!");
    println!("  📡 建议使用≤1472字节以确保网络兼容性");
    
    // 边界值测试
    println!("\n边界值测试:");
    
    // RaptorQ最大符号长度测试
    let max_raptorq_symbol = u16::MAX;
    let alignment_8 = max_raptorq_symbol % 8 == 0;
    println!("  RaptorQ最大符号长度 {} 是否8字节对齐: {}", 
        max_raptorq_symbol, if alignment_8 { "是" } else { "否" });
    
    if !alignment_8 {
        let aligned_max = (max_raptorq_symbol / 8) * 8;
        println!("    建议最大对齐符号长度: {}", aligned_max);
    }
    
    // Reed Solomon GF28最大配置
    println!("  GF28最大安全配置: 200源符号 + 55冗余符号 = 255");
    println!("  GF28理论最大块数: {} (u8::MAX)", u8::MAX);
}