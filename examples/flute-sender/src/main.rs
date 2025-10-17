use flute::{
    core::lct::Cenc,
    core::Oti,
    core::UDPEndpoint,
    sender::{Config as SenderConfig, ObjectDesc, Sender},
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use std::{net::UdpSocket, time::SystemTime};

#[derive(Debug, Deserialize)]
struct AppConfig {
    sender: SenderConfigSection,
}

#[derive(Debug, Deserialize)]
struct SenderConfigSection {
    network: SenderNetworkConfig,
    fec: SenderFecConfig,
    flute: SenderFluteConfig,
    logging: SenderLoggingConfig,
    files: Vec<FileConfig>,
    // New param
    max_rate_kbps: Option<u32>,        // 最大速率限制 (kbps)
}

#[derive(Debug, Deserialize)]
struct SenderNetworkConfig {
    destination: String,
    bind_address: String,
    bind_port: u16,
    send_interval_micros: u64,
    // 单向传输配置
    destination_mac: Option<String>,
    interface: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SenderFecConfig {
    #[serde(rename = "type")]
    fec_type: String,
    encoding_symbol_length: u16,
    max_number_of_parity_symbols: u32,
    maximum_source_block_length: u32,
    symbol_alignment: u8,
    sub_blocks_length: u16,
}

#[derive(Debug, Deserialize)]
struct SenderFluteConfig {
    tsi: u32,
    interleave_blocks: u32,
}

#[derive(Debug, Deserialize)]
struct SenderLoggingConfig {
    progress_interval: u32,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    path: String,
    content_type: String,
    priority: u8,
    version: u32,
}

/// 配置静态ARP表，用于单向传输
fn configure_static_arp(ip: &str, mac: &str, interface: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("🔧 配置Windows11单向传输静态ARP: {} -> {} 在接口 {}", ip, mac, interface);

    // Windows系统使用netsh命令指定接口配置静态ARP

    // 1. 先删除可能存在的旧ARP条目（确保干净的状态）
    log::info!("删除指定接口 '{}' 上的旧ARP条目...", interface);
    let _ = Command::new("netsh")
        .args(&["interface", "ipv4", "delete", "neighbors", interface, ip])
        .output();
    
    // 2. 使用netsh命令在指定接口上添加永久静态ARP条目
    log::info!("在指定接口 '{}' 上添加静态ARP条目: {} -> {}", interface, ip, mac);
    let output = Command::new("netsh")
        .args(&["interface", "ipv4", "add", "neighbors", interface, ip, mac])
        .output()?;

    // 检查命令执行结果
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    if !output.status.success() {
        // 如果是"对象已存在"错误，视为成功
        if stderr.contains("对象已存在") || stdout.contains("对象已存在") {
            log::info!("ℹ️  ARP条目已存在，跳过添加步骤");
        } else {
            log::error!("❌ netsh命令执行失败:");
            log::error!("   退出码: {}", output.status);
            log::error!("   标准输出: {}", stdout);
            log::error!("   错误输出: {}", stderr);
            return Err(format!("配置Windows静态ARP失败: 退出码={}, stderr={}", output.status, stderr).into());
        }
    } else {
        log::info!("✅ netsh命令执行成功");
    }

    // 3. Windows单向传输配置说明
    log::info!("🚫 Windows11单向传输配置提示:");
    log::info!("   1. 已配置静态ARP表项到指定接口");
    log::info!("   2. Windows防火墙可能需要手动配置");
    log::info!("   3. 建议在Windows防火墙中允许FLUTE程序");
    log::info!("   4. 请确保以管理员权限运行程序");

    // 4. 验证静态ARP条目在正确接口上
    log::info!("验证指定接口 '{}' 上的ARP表项...", interface);
    let verify = Command::new("netsh")
        .args(&["interface", "ipv4", "show", "neighbors", interface])
        .output()?;

    let neighbor_output = String::from_utf8_lossy(&verify.stdout);
    if neighbor_output.contains(ip) && neighbor_output.contains(mac) {
        log::info!("✅ Windows静态ARP配置成功: {} -> {} (接口: {})", ip, mac, interface);
        // 查找并显示具体的邻居条目
        for line in neighbor_output.lines() {
            if line.contains(ip) {
                log::info!("📋 邻居表项: {}", line.trim());
                break;
            }
        }
    } else {
        log::warn!("⚠️  ARP验证失败，请检查配置");
        log::debug!("邻居表内容:\n{}", neighbor_output);
    }

    Ok(())
}

fn load_config(config_path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
    log::debug!("Loading configuration from: {}", config_path.display());
    
    if !config_path.exists() {
        return Err(format!("配置文件不存在: {}", config_path.display()).into());
    }
    
    let config_str = fs::read_to_string(config_path)?;
    
    // 调试：显示配置文件内容摘要
    println!("DEBUG: Config file size: {} bytes", config_str.len());
    if let Some(fec_line) = config_str.lines().find(|line| line.contains("encoding_symbol_length")) {
        println!("DEBUG: Found FEC line: {}", fec_line.trim());
    }
    
    let config: AppConfig = serde_yaml::from_str(&config_str)
        .map_err(|e| format!("配置文件解析失败: {}", e))?;
    
    // 配置文件基础验证
    validate_config(&config)?;
    
    Ok(config)
}

fn validate_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let fec = &config.sender.fec;
    
    // 基础参数范围检查
    if fec.encoding_symbol_length == 0 {
        return Err("encoding_symbol_length 不能为0".into());
    }
    
    if fec.maximum_source_block_length == 0 {
        return Err("maximum_source_block_length 不能为0".into());
    }
    
    // UDP限制检查
    const UDP_MAX_PAYLOAD: u16 = 65507;
    if fec.encoding_symbol_length > UDP_MAX_PAYLOAD {
        return Err(format!(
            "encoding_symbol_length ({}) 超过UDP最大载荷限制 {} 字节", 
            fec.encoding_symbol_length, UDP_MAX_PAYLOAD
        ).into());
    }
    
    // FEC特定验证
    match fec.fec_type.as_str() {
        "raptorq" => {
            if (fec.encoding_symbol_length % fec.symbol_alignment as u16) != 0 {
                return Err(format!(
                    "RaptorQ符号对齐错误: encoding_symbol_length ({}) 必须是 symbol_alignment ({}) 的倍数",
                    fec.encoding_symbol_length, fec.symbol_alignment
                ).into());
            }
        },
        "reed_solomon_gf28" => {
            if fec.maximum_source_block_length + fec.max_number_of_parity_symbols > 255 {
                return Err(format!(
                    "Reed Solomon GF28编码块总长度 ({}) 超过255限制", 
                    fec.maximum_source_block_length + fec.max_number_of_parity_symbols
                ).into());
            }
        },
        _ => {} // 其他FEC方案的验证可以在这里添加
    }
    
    log::info!("✅ 配置文件验证通过");
    Ok(())
}

/// RaptorQ传输长度限制验证
/// 根据 oti.rs 中的 max_transfer_length() 和 max_source_blocks_number() 进行验证
fn validate_raptorq_transfer_limits(
    encoding_symbol_length: u16,
    maximum_source_block_length: u32,
    max_number_of_parity_symbols: u16,
    sub_blocks_length: u16,
    symbol_alignment: u8,
) {
    // RaptorQ限制常量 (来自 oti.rs) - 已更新为48位
    const RAPTORQ_MAX_TRANSFER_LENGTH: usize = 0xFFFFFFFFFFFF; // 48 bits max (与其他FEC方案一致)
    const RAPTORQ_MAX_SOURCE_BLOCKS: usize = u8::MAX as usize; // 255

    // 计算单个源块的大小 (字节)
    let block_size = encoding_symbol_length as usize * maximum_source_block_length as usize;
    
    // 计算理论最大传输长度
    let theoretical_max_size = block_size * RAPTORQ_MAX_SOURCE_BLOCKS;
    let actual_max_transfer_length = if theoretical_max_size > RAPTORQ_MAX_TRANSFER_LENGTH {
        RAPTORQ_MAX_TRANSFER_LENGTH
    } else {
        theoretical_max_size
    };

    log::info!("=== RaptorQ传输限制验证 (48位支持) ===");
    log::info!("  单个源块大小: {} bytes ({:.2} MB)", block_size, block_size as f64 / (1024.0 * 1024.0));
    log::info!("  最大源块数量: {} (u8::MAX)", RAPTORQ_MAX_SOURCE_BLOCKS);
    log::info!("  理论最大传输: {} bytes ({:.2} TB)", theoretical_max_size, theoretical_max_size as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0));
    log::info!("  实际传输限制: {} bytes ({:.2} TB)", actual_max_transfer_length, actual_max_transfer_length as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0));
    
    // 验证符号对齐
    if (encoding_symbol_length % symbol_alignment as u16) != 0 {
        panic!("❌ RaptorQ符号对齐验证失败: encoding_symbol_length ({}) 必须是 symbol_alignment ({}) 的倍数", 
               encoding_symbol_length, symbol_alignment);
    }
    
    // 验证编码块长度
    let encoding_block_length = maximum_source_block_length + max_number_of_parity_symbols as u32;
    if encoding_block_length > u16::MAX as u32 {
        panic!("❌ RaptorQ编码块长度验证失败: 总编码块长度 ({}) 超过u16::MAX ({})", 
               encoding_block_length, u16::MAX);
    }
    
    // 验证子块数量
    if sub_blocks_length == 0 {
        panic!("❌ RaptorQ子块长度验证失败: sub_blocks_length 不能为0");
    }

    // 验证参数合理性 - 对于1GB文件的建议
    if block_size > 200 * 1024 * 1024 { // 200MB per block
        log::warn!("⚠️  警告: 单个源块大小 ({:.1} MB) 较大，可能影响内存使用", block_size as f64 / (1024.0 * 1024.0));
    }
    
    log::info!("✅ RaptorQ传输限制验证通过");
    log::info!("========================");
}

fn main() {

    std::env::set_var("RUST_LOG", "info");
    env_logger::builder().try_init().ok();

    // 使用变量选择配置文件
    let config_choice = 19;  // 修改这个数字来选择不同的配置文件 (1-12) - config_1024mb_no_code_1.yaml

    
    let config_paths = vec![
        // 虚拟网卡测试配置 (veth: 192.168.100.1 -> 192.168.100.2)
        "../../config/config_1mb_no_code.yaml", // 1
        "../../config/config_1024mb_no_code.yaml",  // 2
        "../../config/config_1024mb_raptor.yaml",   // 3
        "../../config/config_1024mb_raptorq.yaml", // 4
        "../../config/config_1024mb_reed_solomon_rs28.yaml",  // 5
        "../../config/config_1024mb_reed_solomon_rs28_under_specified.yaml", // 6
        
        // 硬件测试配置 (硬件: 192.168.1.103 -> 192.168.1.102) 
        // No-code
        "../../config/no_code/config_1mb_no_code_1.yaml", // 7
        "../../config/no_code/config_50mb_no_code_1.yaml",  // 8
        "../../config/no_code/config_100mb_no_code_1.yaml",  // 9
        "../../config/no_code/config_200mb_no_code_1.yaml",  // 10
        "../../config/no_code/config_300mb_no_code_1.yaml", // 11
        "../../config/no_code/config_500mb_no_code_1.yaml", // 12
        "../../config/no_code/config_1024mb_no_code_1.yaml", // 13
        // RaptorQ 
        "../../config/raptorq/config_1mb_raptorq_1.yaml", // 14
        "../../config/raptorq/config_50mb_raptorq_1.yaml",  // 15
        "../../config/raptorq/config_100mb_raptorq_1.yaml",  // 16
        "../../config/raptorq/config_200mb_raptorq_1.yaml", // 17 
        "../../config/raptorq/config_300mb_raptorq_1.yaml", // 18 
        "../../config/raptorq/config_500mb_raptorq_1.yaml", // 19
        "../../config/raptorq/config_1024mb_raptorq_1.yaml", // 20 
        // Raptor 
        "../../config/raptor/config_1024mb_raptor_1.yaml", // 21
        // Reed-Solomon
        "../../config/reed-solomon/config_1024mb_reed_solomon_rs28_1.yaml", // 22
        "../../config/reed-solomon/config_1024mb_reed_solomon_rs28_under_specified_1.yaml", // 23
    ];

    if config_choice < 1 || config_choice > config_paths.len() {
        eprintln!("Invalid choice {}, must be 1..{}", config_choice, config_paths.len());
        std::process::exit(1);
    }

    let config_path = Path::new(config_paths[config_choice - 1]);
    
    // 调试输出：显示选择的配置文件
    println!("DEBUG: config_choice = {}", config_choice);
    println!("DEBUG: selected config path = {}", config_path.display());

    let config = match load_config(&config_path) {
        Ok(cfg) => {
            log::info!("Using configuration file: {}", config_path.display());
            cfg
        }
        Err(e) => {
            eprintln!(
                "Failed to load config from {}: {}",
                config_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    // 🔧 配置静态ARP（单向传输关键步骤）
    // 检查是否启用静态ARP配置（便于本地虚拟网卡测试时跳过）
    let enable_static_arp = std::env::var("ENABLE_STATIC_ARP").unwrap_or_else(|_| "true".to_string()).to_lowercase() == "true";
    let manual_arp = std::env::var("MANUAL_ARP_CONFIG").is_ok();
    let skip_arp = std::env::var("SKIP_ARP_CONFIG").is_ok();
    let disable_arp = std::env::var("DISABLE_ARP_REQUESTS").is_ok();

    if !enable_static_arp {
        log::info!("⏭️  跳过静态ARP配置 (ENABLE_STATIC_ARP=false)");
        log::info!("💡 适用于本地虚拟网卡测试环境");
    } else if manual_arp {
        log::info!("🔧 使用手动ARP配置模式 (MANUAL_ARP_CONFIG环境变量已设置)");
        log::info!("💡 请确保已手动执行以下命令:");
        log::info!("   netsh interface ipv4 delete neighbors \"以太网\" 192.168.1.103");
        log::info!("   netsh interface ipv4 add neighbors \"以太网\" 192.168.1.103 10-7c-61-10-a5-47");
        log::info!("💡 验证命令: netsh interface ipv4 show neighbors \"以太网\" | findstr \"192.168.1.103\"");
        log::info!("✅ 程序将完全跳过ARP配置步骤");
    } else if let (Some(dest_mac), Some(interface)) = (
        config.sender.network.destination_mac.as_ref(),
        config.sender.network.interface.as_ref()
    ) {
        let dest_ip = config.sender.network.destination.split(':').next().unwrap();

        if disable_arp {
            log::info!("🚫 ARP请求已禁用，进入纯单向传输模式");
            log::info!("💡 请确保已手动配置静态ARP: {} -> {}", dest_ip, dest_mac);
            log::info!("💡 验证命令: netsh interface ipv4 show neighbors \"{}\" | findstr \"{}\"", interface, dest_ip);
        } else if skip_arp {
            log::info!("⏩ 跳过自动ARP配置 (SKIP_ARP_CONFIG环境变量已设置)");
            log::info!("💡 请确保已手动配置静态ARP: {} -> {}", dest_ip, dest_mac);
            log::info!("💡 验证命令: netsh interface ipv4 show neighbors \"{}\" | findstr \"{}\"", interface, dest_ip);
        } else {
            log::info!("🚀 检测到单向传输配置，正在配置静态ARP...");
            if let Err(e) = configure_static_arp(dest_ip, dest_mac, interface) {
                log::error!("❌ 配置发送端ARP失败: {}", e);
                if cfg!(target_os = "windows") {
                    log::error!("提示: 请以管理员身份运行程序");
                    log::error!("💡 解决方法: 右键点击 PowerShell/命令提示符 → '以管理员身份运行'");
                    log::error!("💡 或者手动配置ARP后设置环境变量: MANUAL_ARP_CONFIG=1");
                } else {
                    log::error!("提示: 确保以sudo权限运行程序");
                }
                std::process::exit(1);
            }
            log::info!("✅ 静态ARP配置成功！");
        }
    } else {
        log::info!("ℹ️  未检测到单向传输配置，跳过ARP设置");
    }

    // 计算所有文件的总原始大小
    let total_file_size: usize = config.sender.files.iter()
        .map(|f| {
            fs::metadata(&f.path)
                .map(|m| m.len() as usize)
                .unwrap_or(0)
        })
        .sum();

    log::info!("Total file size to transmit: {} bytes ({} MB)",
    total_file_size,
    total_file_size as f64 / (1024.0 * 1024.0));

    let endpoint = UDPEndpoint::new(
        None,
        config.sender.network.bind_address.clone(),
        config.sender.network.bind_port,
    );


    // 更安全且带日志的绑定与 connect
    let bind_addr = format!(
        "{}:{}",
        config.sender.network.bind_address, config.sender.network.bind_port
    );
    log::info!("Trying to bind UDP socket to {}", bind_addr);

    let udp_socket = match UdpSocket::bind(&bind_addr) {
        Ok(s) => {
            log::info!("Successfully bound UDP socket to {}", bind_addr);
            match s.local_addr() {
                Ok(local) => log::info!("Socket local_addr() -> {}", local),
                Err(e) => log::warn!("Could not get socket local_addr(): {}", e),
            }
            // 非阻塞 / 其他 socket 设置可以在这里添加，例如：
            // s.set_nonblocking(true).ok();
            s
        }
        Err(e) => {
            log::error!("Failed to bind UDP socket to {}: {}", bind_addr, e);
            // 根据你的需求可以改成 return Err(...) 或者重试逻辑
            std::process::exit(1);
        }
    };

    // 打印将要连接的目的地址（目标必须包含端口，例如 "192.168.1.102:12345"）
    log::info!("Will connect UDP socket to destination: {}", config.sender.network.destination);

    match udp_socket.connect(&config.sender.network.destination) {
        Ok(_) => {
            match udp_socket.peer_addr() {
                Ok(peer) => log::info!("UDP socket connected to {}", peer),
                Err(_) => log::info!("UDP socket connected (peer addr not available)"),
            }
        }
        Err(e) => {
            log::error!(
                "Failed to connect UDP socket to {}: {}",
                config.sender.network.destination,
                e
            );
            std::process::exit(1);
        }
    }

    let tsi = config.sender.flute.tsi;

    // 从配置文件加载FEC参数 - 严格参数验证，无回退机制
    let max_number_of_parity_symbols: u16 = config.sender.fec.max_number_of_parity_symbols.try_into()
        .expect(&format!("max_number_of_parity_symbols ({}) 超出u16范围", config.sender.fec.max_number_of_parity_symbols));
    
    let encoding_symbol_length: u16 = config.sender.fec.encoding_symbol_length.try_into()
        .expect(&format!("encoding_symbol_length ({}) 超出u16范围", config.sender.fec.encoding_symbol_length));
    
    let max_source_block_length = config.sender.fec.maximum_source_block_length;
    let symbol_alignment = config.sender.fec.symbol_alignment;
    let sub_blocks_length = config.sender.fec.sub_blocks_length;

    // UDP数据包大小预检查 - 防止运行时错误
    const UDP_MAX_PAYLOAD: u16 = 65507;
    if encoding_symbol_length > UDP_MAX_PAYLOAD {
        panic!("编码符号长度 {} 超过UDP最大载荷限制 {} 字节，这将导致 'Message too long' 错误！", 
               encoding_symbol_length, UDP_MAX_PAYLOAD);
    }

    log::info!("参数验证通过:");
    log::info!("  - 编码符号长度: {} 字节 (UDP限制: ≤{})", encoding_symbol_length, UDP_MAX_PAYLOAD);
    log::info!("  - 最大源块长度: {} 符号", max_source_block_length);
    log::info!("  - 最大冗余符号: {} 符号", max_number_of_parity_symbols);
    log::info!("  - 符号对齐: {} 字节", symbol_alignment);
    log::info!("  - 子块长度: {}", sub_blocks_length);
    
    // 输出速率控制参数
    if let Some(max_rate) = config.sender.max_rate_kbps {
        log::info!("  - 最大速率限制: {} kbps ({:.2} Mbps)", max_rate, max_rate as f32 / 1000.0);
    } else {
        log::info!("  - 最大速率限制: 无限制");
    }
    
    let interval = config.sender.network.send_interval_micros;
    log::info!("  - 发送间隔: {} 微秒", interval);

    // 使用配置文件中的FEC类型，但参数仍使用硬编码进行测试
    log::info!("配置文件FEC类型: {}", config.sender.fec.fec_type);
    let oti = match config.sender.fec.fec_type.as_str() {
        "no_code" => Oti::new_no_code(encoding_symbol_length, max_source_block_length.try_into().unwrap()),
        "reed_solomon_gf28" => Oti::new_reed_solomon_rs28(
            encoding_symbol_length,
            max_source_block_length.try_into().unwrap(),
            max_number_of_parity_symbols as u8,
        ).expect("Invalid Reed Solomon GF28 parameters"),
        "reed_solomon_gf28_under_specified" => Oti::new_reed_solomon_rs28_under_specified(
            encoding_symbol_length,
            max_source_block_length.try_into().unwrap(),
            max_number_of_parity_symbols,
        ).expect("Invalid Reed Solomon GF28 Under Specified parameters"),
        "raptor" => Oti::new_raptor(
            encoding_symbol_length,
            max_source_block_length.try_into().unwrap(),
            max_number_of_parity_symbols,
            sub_blocks_length.try_into().unwrap(),
            symbol_alignment,
        ).expect("Invalid Raptor parameters"),
        "raptorq" => {
            // RaptorQ特殊验证 - 符号对齐检查
            if (encoding_symbol_length % symbol_alignment as u16) != 0 {
                panic!("RaptorQ符号对齐错误: encoding_symbol_length ({}) 必须是 symbol_alignment ({}) 的倍数", 
                       encoding_symbol_length, symbol_alignment);
            }

            // RaptorQ传输长度限制验证
            validate_raptorq_transfer_limits(
                encoding_symbol_length,
                max_source_block_length,
                max_number_of_parity_symbols,
                sub_blocks_length,
                symbol_alignment
            );
            
            Oti::new_raptorq(
                encoding_symbol_length,
                max_source_block_length.try_into()
                    .expect(&format!("max_source_block_length ({}) 超出u16范围", max_source_block_length)),
                max_number_of_parity_symbols,
                sub_blocks_length,
                symbol_alignment,
            ).unwrap_or_else(|e| {
                panic!("RaptorQ参数验证失败: {:?}\n参数: symbol_length={}, block_length={}, parity={}, sub_blocks={}, alignment={}", 
                       e, encoding_symbol_length, max_source_block_length, max_number_of_parity_symbols, sub_blocks_length, symbol_alignment);
            })
        },
        _ => panic!("Unsupported FEC type: {}", config.sender.fec.fec_type),
    };

    log::info!("Using FEC: {:?}", oti.fec_encoding_id);
    log::info!("Encoding symbol length: {} bytes", oti.encoding_symbol_length);
    log::info!("Max source block length: {}", oti.maximum_source_block_length);
    log::info!("Sub blocks length: {}", sub_blocks_length);
    log::info!("Max parity symbols: {}", oti.max_number_of_parity_symbols);
    log::info!("Max symbol alignment: {}", oti.max_number_of_parity_symbols);
    let mut sender_config = SenderConfig::default();
    sender_config.interleave_blocks = config.sender.flute.interleave_blocks.try_into().unwrap();

    let mut sender = Sender::new(endpoint, tsi.into(), &oti, &sender_config);

    udp_socket
        .connect(&config.sender.network.destination)
        .unwrap();


    for file_config in &config.sender.files {
        let path = Path::new(&file_config.path);
        if !path.is_file() {
            log::error!("File not found: {}", file_config.path);
            continue;
        }

        // 获取文件大小并验证传输限制
        let file_size = path.metadata().unwrap().len() as usize;
        let max_transfer_length = oti.max_transfer_length();
        
        log::info!("Insert file {} to FLUTE sender", file_config.path);
        log::info!("文件大小: {} bytes ({:.2} MB)", file_size, file_size as f64 / (1024.0 * 1024.0));
        log::info!("传输限制: {} bytes ({:.2} MB)", max_transfer_length, max_transfer_length as f64 / (1024.0 * 1024.0));
        
        if file_size > max_transfer_length {
            panic!("❌ 文件传输限制验证失败!\n文件大小: {} bytes ({:.2} MB)\n传输限制: {} bytes ({:.2} MB)\n请调整FEC参数以支持更大文件传输", 
                   file_size, file_size as f64 / (1024.0 * 1024.0),
                   max_transfer_length, max_transfer_length as f64 / (1024.0 * 1024.0));
        }
        
        log::info!("✅ 文件大小验证通过 ({:.1}% of limit)", (file_size as f64 / max_transfer_length as f64) * 100.0);

        let obj = ObjectDesc::create_from_file(
            path,
            None,
            &file_config.content_type,
            true,
            file_config.version,
            None,
            None,
            None,
            None,
            Cenc::Null,
            true,
            None,
            true,
        )
            .unwrap();
        sender.add_object(file_config.priority.into(), obj).unwrap();
    }

    sender.publish(SystemTime::now()).unwrap();

    log::info!("Starting file transmission...");
    let start_time = Instant::now();
    let mut total_bytes_sent: u64 = 0;
    let mut sent_packets: u64 = 0;

    let send_interval_micros = config.sender.network.send_interval_micros;
    let max_rate_kbps = config.sender.max_rate_kbps.unwrap_or(0);
    let bytes_per_sec = if max_rate_kbps > 0 {
        max_rate_kbps as f64 * 1000.0 / 8.0 // kbps -> Bps
    } else {
        f64::INFINITY // 不限速
    };

    if send_interval_micros > 0 {
        log::info!("Rate control: send_interval_micros = {} ({} us per packet)", send_interval_micros, send_interval_micros);
    } else {
        log::info!("Rate control: max_rate_kbps = {} ({} B/s)",
           max_rate_kbps, if bytes_per_sec.is_finite() { bytes_per_sec as u64 } else { 0 });
    }

    // 用“下一次应发送时间”做节拍
    let mut next_send_at = Instant::now();

    // 日志辅助
    let mut last_log_time = Instant::now();
    let mut bytes_since_log: u64 = 0;
    let mut packets_since_log: u64 = 0;

    while let Some(pkt) = sender.read(SystemTime::now()) {
        if send_interval_micros > 0 {
            std::thread::sleep(Duration::from_micros(send_interval_micros));
        } else if bytes_per_sec.is_finite() {
            let pkt_len = pkt.len() as f64;
            let interval = Duration::from_secs_f64(pkt_len / bytes_per_sec);
            let now = Instant::now();
            if now < next_send_at {
                std::thread::sleep(next_send_at - now);
            }
            next_send_at += interval;
            let drift = Instant::now().saturating_duration_since(next_send_at);
            if drift > Duration::from_millis(200) {
                next_send_at = Instant::now() + interval;
            }
        }

        match udp_socket.send(&pkt) {
            Ok(bytes_sent) => {
                total_bytes_sent += bytes_sent as u64;
                bytes_since_log += bytes_sent as u64;
                packets_since_log += 1;
                sent_packets += 1;

                // 按进度间隔打印统计
                if sent_packets % config.sender.logging.progress_interval as u64 == 0 {
                    let now = Instant::now();
                    let dt = now.duration_since(last_log_time).as_secs_f64();
                    if dt > 0.0 {
                        let inst_mbps = (bytes_since_log as f64 * 8.0) / dt / 1_000_000.0;
                        let avg_mbps = (total_bytes_sent as f64 * 8.0)
                            / now.duration_since(start_time).as_secs_f64() / 1_000_000.0;
                        let pps = packets_since_log as f64 / dt;

                        log::info!(
                        "Progress: {} pkts, {} MB | Instant: {:.2} Mbps | Avg: {:.2} Mbps | PPS: {:.0}",
                        sent_packets,
                        total_bytes_sent / (1024 * 1024),
                        inst_mbps,
                        avg_mbps,
                        pps
                    );
                    }
                    last_log_time = now;
                    bytes_since_log = 0;
                    packets_since_log = 0;
                }
            }
            Err(e) => {
                log::error!("Failed to send packet: {}", e);
            }
        }

        // ✅ 这里不要再做额外的 sleep（删除你原来的 network.send_interval_micros 睡眠）
    }

    // 传输完成后的详细统计
    let total_time = start_time.elapsed();
    let total_mb_sent = total_bytes_sent as f64 / (1024.0 * 1024.0);
    let total_mb_recv = total_file_size as f64 / (1024.0 * 1024.0);

    // 总共传输文件大小除以用时
    let average_rate_mbps_sender = (total_bytes_sent as f64 * 8.0) / total_time.as_secs_f64() / 1_000_000.0;
    // 原始文件大小除以用时
    let average_rate_mbps_receiver = (total_file_size as f64 * 8.0) / total_time.as_secs_f64() / 1_000_000.0;

    log::info!("==========================================");
    log::info!("FILE TRANSFER COMPLETED");
    log::info!("==========================================");
    log::info!("Total time: {:.2} seconds", total_time.as_secs_f64());
    log::info!("Total packets: {}", sent_packets);
    log::info!("Total data sent: {:.2} MB", total_mb_sent);
    log::info!("Total data received: {:.2} MB", total_mb_recv);
    log::info!("Average rate for sender: {:.2} Mbps", average_rate_mbps_sender);
    log::info!("Average rate for sender: {:.2} MB/s", average_rate_mbps_sender / 8.0);
    log::info!("Average rate for receiver: {:.2} Mbps", average_rate_mbps_receiver);
    log::info!("Average rate for receiver: {:.2} MB/s", average_rate_mbps_receiver / 8.0);
    log::info!("Packet rate: {:.2} packets/second",
               sent_packets as f64 / total_time.as_secs_f64());
    log::info!("==========================================");
    log::info!(
        "File transfer completed. Total packets sent: {}",
        sent_packets
    );
}