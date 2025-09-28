use flute::{
    core::UDPEndpoint,
    receiver::{writer, Config as ReceiverConfig, MultiReceiver},
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::SystemTime;

#[derive(Debug, Deserialize)]
struct AppConfig {
    receiver: ReceiverConfigSection,
}

#[derive(Debug, Deserialize)]
struct ReceiverConfigSection {
    network: ReceiverNetworkConfig,
    storage: ReceiverStorageConfig,
    logging: ReceiverLoggingConfig,
    advanced: ReceiverAdvancedConfig,
}

#[derive(Debug, Deserialize)]
struct ReceiverNetworkConfig {
    bind_address: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
struct ReceiverStorageConfig {
    destination_dir: String,
    enable_md5_check: bool,
}

#[derive(Debug, Deserialize)]
struct ReceiverLoggingConfig {
    progress_interval: u32,
}

#[derive(Debug, Deserialize)]
struct ReceiverAdvancedConfig {
    buffer_size: usize,
    cleanup_interval: u32,
    log_interval: u32,
    max_memory_mb: u64,
    #[serde(default = "default_true")]
    keep_partial_files: bool,
}

fn default_true() -> bool {
    true
}

fn load_config(config_path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
    log::debug!("Loading configuration from: {}", config_path.display());
    let config_str = fs::read_to_string(config_path)?;
    let config: AppConfig = serde_yaml::from_str(&config_str)?;
    Ok(config)
}


fn main() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::builder().try_init().ok();

    // 使用变量选择配置文件  
    let config_choice = 2;  // 修改这个数字来选择不同的配置文件 (1-12) - 与发送端保持一致
    
    // 配置文件列表，使用绝对路径
    // 1-6: 虚拟网卡测试配置 (veth-receiver: 192.168.100.2)
    // 7-12: 硬件测试配置 (win11 接收端: 192.168.1.102)
    let config_paths = vec![
        // 虚拟网卡测试配置 (veth-receiver: 192.168.100.2)
        "/home/Halllo/Projects/flute/examples/config/config_1mb_no_code.yaml",                            // 1
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_no_code.yaml",                        // 2
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_raptor.yaml",                         // 3
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_raptorq.yaml",                        // 4
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_reed_solomon_rs28.yaml",              // 5
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_reed_solomon_rs28_under_specified.yaml", // 6
        // 硬件测试配置 (win11 接收端: 192.168.1.102)
        "/home/Halllo/Projects/flute/examples/config/config_1mb_no_code_1.yaml",                         // 7
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_no_code_1.yaml",                      // 8
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_raptor_1.yaml",                       // 9
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_raptorq_1.yaml",                      // 10
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_reed_solomon_rs28_1.yaml",            // 11
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_reed_solomon_rs28_under_specified_1.yaml", // 12
    ];
    
    if config_choice < 1 || config_choice > config_paths.len() {
        eprintln!("Invalid choice {}, must be 1..{}", config_choice, config_paths.len());
        std::process::exit(1);
    }
    
    let config_path = Path::new(config_paths[config_choice - 1]);

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

    let endpoint = UDPEndpoint::new(
        None,
        config.receiver.network.bind_address.clone(),
        config.receiver.network.port,
    );

    let dest_dir = Path::new(&config.receiver.storage.destination_dir);
    if !dest_dir.is_dir() {
        if let Err(e) = std::fs::create_dir_all(dest_dir) {
            log::error!("Failed to create directory {:?}: {}", dest_dir, e);
            std::process::exit(-1);
        }
        log::info!("Created destination directory: {:?}", dest_dir);
    }

    log::info!("Create FLUTE receiver, writing objects to {:?}", dest_dir);
    
    // 输出接收端配置参数
    log::info!("=== 接收端配置参数 ===");
    log::info!("  - 监听地址: {}:{}", config.receiver.network.bind_address, config.receiver.network.port);
    log::info!("  - 目标目录: {:?}", dest_dir);
    log::info!("  - 缓冲区大小: {} KB", config.receiver.advanced.buffer_size / 1024);
    log::info!("  - 最大内存限制: {} MB", config.receiver.advanced.max_memory_mb);
    log::info!("  - 清理间隔: {} packets", config.receiver.advanced.cleanup_interval);
    log::info!("  - 日志间隔: {} packets", config.receiver.advanced.log_interval);
    log::info!("  - MD5检查: {}", config.receiver.storage.enable_md5_check);
    log::info!("====================");

    let mut receiver_config = ReceiverConfig::default();
    receiver_config.object_max_cache_size = Some(config.receiver.advanced.max_memory_mb as usize * 1024 * 1024);

    let writer = match writer::ObjectWriterFSBuilder::new(dest_dir, config.receiver.storage.enable_md5_check) {
        Ok(builder) => Rc::new(builder),
        Err(e) => {
            log::error!("Failed to create writer: {:?}", e);
            std::process::exit(1);
        }
    };

    let mut receiver = MultiReceiver::new(writer, Some(receiver_config), false);

    let socket = match std::net::UdpSocket::bind(format!(
        "{}:{}",
        config.receiver.network.bind_address, config.receiver.network.port
    )) {
        Ok(socket) => socket,
        Err(e) => {
            log::error!("Failed to bind UDP socket: {}", e);
            std::process::exit(1);
        }
    };

    // 直接使用配置参数初始化 buf，无需额外 UDP 缓冲区设置

    log::info!(
        "Listening on port {} for FLUTE data",
        config.receiver.network.port
    );

    let mut buf = vec![0; config.receiver.advanced.buffer_size];
    let mut received_packets = 0;
    let max_memory_bytes = config.receiver.advanced.max_memory_mb * 1024 * 1024;
    let mut memory_usage: u64 = 0;
    let mut packet_errors = 0;
    let start_time = std::time::Instant::now();

    log::info!("🚀 Starting packet reception loop...");

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                received_packets += 1;
                memory_usage += n as u64;

                // 🧠 智能内存管理 - 分级清理策略
                let memory_usage_mb = memory_usage / (1024 * 1024);
                let memory_limit_mb = config.receiver.advanced.max_memory_mb;
                
                if memory_usage_mb > memory_limit_mb {
                    log::warn!(
                        "🚨 Memory usage {}MB exceeds limit {}MB, forcing cleanup",
                        memory_usage_mb, memory_limit_mb
                    );
                    let now = SystemTime::now();
                    receiver.cleanup(now);
                    memory_usage = 0;
                } else if memory_usage_mb > (memory_limit_mb * 3 / 4) {
                    // 75% 时预警但不清理
                    if received_packets % (config.receiver.advanced.log_interval * 10) == 0 {
                        log::info!("⚠️  Memory usage approaching limit: {}MB / {}MB ({:.1}%)", 
                                  memory_usage_mb, memory_limit_mb, 
                                  (memory_usage_mb as f64 / memory_limit_mb as f64) * 100.0);
                    }
                }

                // if received_packets % config.receiver.advanced.log_interval == 0 {
                //     let elapsed_secs = match start_time.elapsed() {
                //         Ok(dur) => dur.as_secs_f64(),
                //         Err(_) => 1.0,
                //     }.max(0.001); // 避免除零
                //     let rate_pps = received_packets as f64 / elapsed_secs;
                //     let rate_mbps = (memory_usage as f64 * 8.0) / (1024.0 * 1024.0) / elapsed_secs;
                //     log::info!("📥 Received {} packets from {} | Memory: {:.1}MB | PacketSize: {} | Rate: {:.0} pps ({:.1} Mbps) | Errors: {}", 
                //               received_packets, src, memory_usage as f64 / (1024.0 * 1024.0), n, rate_pps, rate_mbps, packet_errors);
                // }

                let now = SystemTime::now();
                if let Err(e) = receiver.push(&endpoint, &buf[..n], now) {
                    packet_errors += 1;
                    log::warn!("❌ Packet processing error #{}: {:?}", packet_errors, e);
                    if packet_errors % 100 == 0 {
                        log::error!("🚨 累计包处理错误: {} | 成功包: {} | 错误率: {:.2}%", 
                                   packet_errors, received_packets, 
                                   (packet_errors as f64 / (received_packets + packet_errors) as f64) * 100.0);
                    }
                    continue;
                }

                if config.receiver.advanced.cleanup_interval > 0 &&
                    received_packets % config.receiver.advanced.cleanup_interval == 0
                {
                    let now = SystemTime::now();
                    receiver.cleanup(now);
                    memory_usage = 0;
                }
            }
            Err(e) => {
                log::error!("Receive error: {}", e);
                break;
            }
        }
    }
}

