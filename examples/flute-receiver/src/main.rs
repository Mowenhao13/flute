use flute::{
    core::UDPEndpoint,
    receiver::{writer, Config as ReceiverConfig, MultiReceiver},
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::process::Command;
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
    // 单向传输配置
    sender_mac: Option<String>,
    sender_ip: Option<String>,
    interface: Option<String>,
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

}



/// 更强力的单向传输配置
fn configure_unidirectional_network(ip: &str, mac: &str, interface: &str) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("🔧 配置强制单向传输模式: {} -> {} 在接口 {}", ip, mac, interface);
    
    // 1. 清理所有现有ARP条目
    log::info!("🧹 清理现有ARP表...");
    let _ = Command::new("sudo")
        .args(&["ip", "neigh", "flush", "dev", interface])
        .output();
    
    // 2. 禁用IPv6（减少网络发现流量）
    log::info!("🚫 禁用IPv6...");
    let _ = Command::new("sudo")
        .args(&["sysctl", "-w", "net.ipv6.conf.all.disable_ipv6=1"])
        .output();
    let _ = Command::new("sudo")
        .args(&["sysctl", "-w", &format!("net.ipv6.conf.{}.disable_ipv6=1", interface)])
        .output();
    
    // 3. 禁用网络发现协议
    log::info!("🔇 禁用网络发现协议...");
    let _ = Command::new("sudo")
        .args(&["sysctl", "-w", "net.ipv4.conf.all.send_redirects=0"])
        .output();
    let _ = Command::new("sudo")
        .args(&["sysctl", "-w", "net.ipv4.conf.all.accept_redirects=0"])
        .output();
    
    // 4. 配置静态ARP（永久条目）
    log::info!("🔗 配置永久静态ARP条目...");
    let output = Command::new("sudo")
        .args(&["ip", "neigh", "add", ip, "lladdr", mac, "dev", interface, "nud", "permanent"])
        .output()?;
    
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        if !error.contains("File exists") {
            return Err(format!("配置静态ARP失败: {}", error).into());
        }
        log::info!("ARP条目已存在，继续配置...");
    }
    
    // 5. 强制禁用ARP协议
    log::info!("🚫 强制禁用ARP协议...");
    
    // 禁用ARP请求和响应
    let arp_configs = vec![
        format!("net.ipv4.conf.{}.arp_ignore=2", interface),      // 忽略所有ARP请求
        format!("net.ipv4.conf.{}.arp_announce=2", interface),    // 不发送ARP announce
        format!("net.ipv4.conf.{}.arp_accept=0", interface),      // 不接受ARP信息
        format!("net.ipv4.conf.{}.rp_filter=0", interface),       // 禁用反向路径过滤
        "net.ipv4.conf.all.arp_ignore=2".to_string(),            // 全局禁用ARP响应
        "net.ipv4.conf.all.arp_announce=2".to_string(),          // 全局禁用ARP通告
        "net.ipv4.conf.all.arp_accept=0".to_string(),            // 全局禁用ARP接受
    ];
    
    for config in arp_configs {
        let result = Command::new("sudo")
            .args(&["sysctl", "-w", &config])
            .output();
        match result {
            Ok(output) => {
                if output.status.success() {
                    log::debug!("✅ Applied: {}", config);
                } else {
                    log::warn!("⚠️  Failed to apply: {}", config);
                }
            }
            Err(e) => log::warn!("⚠️  Error applying {}: {}", config, e),
        }
    }
    
    // 6. 验证配置
    log::info!("🔍 验证单向传输配置...");
    let verify = Command::new("ip")
        .args(&["neigh", "show", "dev", interface])
        .output()?;
    
    let arp_output = String::from_utf8_lossy(&verify.stdout);
    if arp_output.contains(ip) {
        log::info!("✅ 静态ARP配置成功: {} -> {}", ip, mac);
        log::info!("📋 ARP表项: {}", arp_output.lines().find(|l| l.contains(ip)).unwrap_or("未找到"));
    } else {
        log::warn!("⚠️  ARP验证失败，但继续尝试接收数据");
        log::debug!("ARP表内容:\n{}", arp_output);
    }
    
    // 7. 检查sysctl配置
    let sysctl_check = Command::new("sysctl")
        .args(&[&format!("net.ipv4.conf.{}.arp_ignore", interface)])
        .output()?;
    
    let sysctl_output = String::from_utf8_lossy(&sysctl_check.stdout);
    log::info!("📋 ARP忽略状态: {}", sysctl_output.trim());
    
    log::info!("🔒 单向传输网络配置完成！");
    
    Ok(())
}

fn load_config(config_path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
    log::debug!("Loading configuration from: {}", config_path.display());
    let config_str = fs::read_to_string(config_path)?;
    let config: AppConfig = serde_yaml::from_str(&config_str)?;
    Ok(config)
}


fn main() {
    std::env::set_var("RUST_LOG", "info");
    env_logger::builder().try_init().ok();

    // 使用变量选择配置文件  
    let config_choice = 19;  // 修改这个数字来选择不同的配置文件 (1-23) - 与发送端保持一致
    
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
        "../../config/reed_solomon/config_1024mb_reed_solomon_rs28_1.yaml", // 22
        "../../config/reed_solomon/config_1024mb_reed_solomon_rs28_under_specified_1.yaml", // 23
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

    // 🔧 配置单向传输网络（更强力的版本）
    // 检查是否启用静态ARP配置（便于本地虚拟网卡测试时跳过）
    let enable_static_arp = std::env::var("ENABLE_STATIC_ARP").unwrap_or_else(|_| "true".to_string()).to_lowercase() == "true";
    
    if !enable_static_arp {
        log::info!("⏭️  跳过单向网络配置 (ENABLE_STATIC_ARP=false)");
        log::info!("💡 适用于本地虚拟网卡测试环境");
    } else if let (Some(sender_mac), Some(sender_ip), Some(interface)) = (
        config.receiver.network.sender_mac.as_ref(),
        config.receiver.network.sender_ip.as_ref(),
        config.receiver.network.interface.as_ref()
    ) {
        log::info!("🚀 检测到单向传输配置，正在配置强制单向网络...");
        if let Err(e) = configure_unidirectional_network(sender_ip, sender_mac, interface) {
            log::error!("❌ 配置单向网络失败: {}", e);
            log::error!("提示: 确保以sudo权限运行程序");
            std::process::exit(1);
        }
    } else {
        log::info!("ℹ️  未检测到单向传输配置，跳过ARP设置");
    }

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
        Ok(socket) => {
            log::info!("✅ UDP socket successfully bound to {}:{}", 
                      config.receiver.network.bind_address, config.receiver.network.port);
            
            // 配置socket参数
            if let Err(e) = socket.set_read_timeout(Some(std::time::Duration::from_secs(1))) {
                log::warn!("Failed to set socket timeout: {}", e);
            }
            
            // 设置非阻塞模式用于更好的错误处理
            if let Err(e) = socket.set_nonblocking(false) {
                log::warn!("Failed to set blocking mode: {}", e);
            }
            
            socket
        }
        Err(e) => {
            log::error!("Failed to bind UDP socket: {}", e);
            std::process::exit(1);
        }
    };

    // 直接使用配置参数初始化 buf，无需额外 UDP 缓冲区设置

    log::info!(
        "🎯 UDP Socket successfully bound to {}:{}",
        config.receiver.network.bind_address, config.receiver.network.port
    );

    let mut buf = vec![0; config.receiver.advanced.buffer_size];
    let mut received_packets = 0;
    let _max_memory_bytes = config.receiver.advanced.max_memory_mb * 1024 * 1024;
    let mut memory_usage: u64 = 0;
    let mut packet_errors = 0;
    let start_time = std::time::Instant::now();

    log::info!("🚀 Starting packet reception loop with buffer size: {} bytes", config.receiver.advanced.buffer_size);
    log::info!("📡 Waiting for packets from {}...", config.receiver.network.sender_ip.as_ref().unwrap_or(&"any".to_string()));

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                // 第一个包特殊处理
                if received_packets == 0 {
                    log::info!("🎉 First packet received! Source: {}, Size: {} bytes", src, n);
                }
                
                received_packets += 1;
                memory_usage += n as u64;

                // 每100个包输出一次状态
                if received_packets % 100 == 0 {
                    let elapsed_secs = start_time.elapsed().as_secs_f64().max(0.001);
                    let rate_pps = received_packets as f64 / elapsed_secs;
                    log::info!("📥 Progress: {} packets, {:.0} pps, {:.1} MB total", 
                              received_packets, rate_pps, memory_usage as f64 / (1024.0 * 1024.0));
                }

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
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                // 超时，继续等待
                log::debug!("Socket timeout, continuing to wait for packets...");
                continue;
            }
            Err(e) => {
                log::error!("❌ Socket receive error: {}", e);
                packet_errors += 1;
                if packet_errors > 1000 {
                    log::error!("🚨 Too many socket errors, exiting");
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
        }
    }
    
    log::info!("🏁 Reception loop ended. Total packets: {}, Errors: {}", received_packets, packet_errors);
}

