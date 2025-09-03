// use flute::{
//     core::UDPEndpoint,
//     receiver::{writer, MultiReceiver},
// };
// use std::rc::Rc;

// fn main() {
//     std::env::set_var("RUST_LOG", "info");
//     env_logger::builder().try_init().ok();
//     // 从命令行参数获取监听端口
//     let args: Vec<String> = std::env::args().collect();
//     if args.len() < 2 {
//         println!("Save FLUTE objects received via unicast");
//         println!("Usage: {} path/to/destination_folder [port]", args[0]);
//         println!("Default port: 3400");
//         std::process::exit(0);
//     }

//     let port = if args.len() > 2 {
//         args[2].parse().unwrap_or(3400)
//     } else {
//         3400
//     };

//     // 使用单播端点
//     let endpoint = UDPEndpoint::new(None, "0.0.0.0".to_string(), port);

//     let dest_dir = std::path::Path::new(&args[1]);
//     if !dest_dir.is_dir() {
//         log::error!("{:?} is not a directory", dest_dir);
//         std::process::exit(-1);
//     }

//     log::info!("Create FLUTE receiver, writing objects to {:?}", dest_dir);

//     let enable_md5_check = true;
//     let writer = Rc::new(writer::ObjectWriterFSBuilder::new(dest_dir, enable_md5_check).unwrap());
//     let mut receiver = MultiReceiver::new(writer, None, false);

//     // 创建普通UDP socket而不是组播socket
//     let socket = std::net::UdpSocket::bind(format!("0.0.0.0:{}", port))
//         .expect("Failed to bind UDP socket");

//     // 设置接收缓冲区大小
//     // socket.set_recv_buffer_size(1024 * 1024).unwrap();

//     log::info!("Listening on port {} for FLUTE data", port);

//     let mut buf = [0; 204800];
//     let mut received_packets = 0;
//     loop {
//         match socket.recv_from(&mut buf) {
//             Ok((n, src)) => {
//                 received_packets += 1;
//                 if received_packets % 100 == 0 {
//                     log::info!("Received {} packets from {}", received_packets, src);
//                 }

//                 let now = std::time::SystemTime::now();
//                 if let Err(e) = receiver.push(&endpoint, &buf[..n], now) {
//                     log::error!("Error processing packet: {:?}", e);
//                 }
//                 receiver.cleanup(now);
//             }
//             Err(e) => {
//                 log::error!("Failed to receive data: {:?}", e);
//             }
//         }
//     }
// }

// flute-receiver/src/main.rs
use flute::{
    core::UDPEndpoint,
    receiver::{writer, MultiReceiver},
};
use std::rc::Rc;
use serde::Deserialize;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::time::{Instant, SystemTime, Duration};
use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::VecDeque;

// 配置结构体
#[derive(Debug, Deserialize)]
struct AppConfig {
    network: NetworkConfig,
    storage: StorageConfig,
    flute: FluteConfig,
    logging: LoggingConfig,
    advanced: AdvancedConfig,
}

#[derive(Debug, Deserialize)]
struct NetworkConfig {
    bind_address: String,
    port: u16,
    buffer_size: usize,
    socket_buffer_size: usize,
}

#[derive(Debug, Deserialize)]
struct StorageConfig {
    destination_dir: String,
    enable_md5_check: bool,
    overwrite_existing: bool,
}

#[derive(Debug, Deserialize)]
struct FluteConfig {
    enable_multireceiver: bool,
    cleanup_interval: u64,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
    show_progress: bool,
    progress_interval: u32,
    log_source_address: bool,
}

#[derive(Debug, Deserialize)]
struct AdvancedConfig {
    max_packet_size: usize,
    receive_timeout: u64,
    enable_statistics: bool,
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let config_str = fs::read_to_string("receiver_config.yaml")?;
    let config: AppConfig = serde_yaml::from_str(&config_str)?;
    Ok(config)
}

fn main() {
    // 加载配置
    let config = match load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    // 设置日志级别
    std::env::set_var("RUST_LOG", &config.logging.level);
    env_logger::builder().try_init().ok();

    log::info!("Starting FLUTE receiver with config: {:?}", config);

    // 使用单播端点
    let endpoint = UDPEndpoint::new(None, config.network.bind_address.clone(), config.network.port);

    let dest_dir = std::path::Path::new(&config.storage.destination_dir);
    if !dest_dir.is_dir() {
        if let Err(e) = std::fs::create_dir_all(dest_dir) {
            log::error!("Failed to create directory {:?}: {}", dest_dir, e);
            std::process::exit(-1);
        }
        log::info!("Created destination directory: {:?}", dest_dir);
    }

    log::info!("Create FLUTE receiver, writing objects to {:?}", dest_dir);

    let writer = Rc::new(writer::ObjectWriterFSBuilder::new(dest_dir, config.storage.enable_md5_check)
        .unwrap_or_else(|e| {
            log::error!("Failed to create writer: {:?}", e);
            std::process::exit(-1);
        }));

    let mut receiver = MultiReceiver::new(writer, None, false);

    // 创建普通UDP socket
    let socket = std::net::UdpSocket::bind(format!("{}:{}", config.network.bind_address, config.network.port))
        .expect("Failed to bind UDP socket");

    // 设置接收缓冲区大小
    unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &config.network.socket_buffer_size as *const _ as *const libc::c_void,
            std::mem::size_of::<i32>() as libc::socklen_t,
        );
    }

    // 设置接收超时
    socket.set_read_timeout(Some(Duration::from_secs(config.advanced.receive_timeout))).unwrap();

    log::info!("Listening on {}:{} for FLUTE data", config.network.bind_address, config.network.port);

    let mut buf = vec![0; config.advanced.max_packet_size];
    let mut received_packets = 0;
    let mut total_bytes = 0;
    let start_time = Instant::now();
    
    // 速率统计相关变量
    let peak_rate = AtomicU64::new(0); // 存储峰值速率（单位：0.01Mbps）
    let mut last_bytes = 0;
    let mut last_update = Instant::now();
    let mut last_report = Instant::now();
    
    // 滑动窗口统计（最近10个采样点）
    const RATE_WINDOW: usize = 10;
    let mut rate_history = VecDeque::with_capacity(RATE_WINDOW);
    let mut last_sample_time = Instant::now();
    let mut last_sample_bytes = 0;

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                received_packets += 1;
                total_bytes += n;

                // ==== 实时速率计算 ====
                let now = Instant::now();
                
                // 瞬时速率计算（每包）
                let elapsed = now - last_update;
                if elapsed.as_secs_f64() > 0.0 {
                    let current_rate = ((total_bytes - last_bytes) as f64 * 8.0) / elapsed.as_secs_f64() / 1_000_000.0;
                    
                    // 更新峰值（保留两位小数）
                    peak_rate.fetch_max((current_rate * 100.0) as u64, Ordering::Relaxed);
                    
                    last_bytes = total_bytes;
                    last_update = now;
                }

                // 滑动窗口采样（每100ms或满包时）
                if now - last_sample_time > Duration::from_millis(100) || n == buf.len() {
                    let elapsed = (now - last_sample_time).as_secs_f64();
                    if elapsed > 0.0 {
                        let rate = ((total_bytes - last_sample_bytes) as f64 * 8.0) / (elapsed * 1_000_000.0);
                        rate_history.push_back(rate);
                        if rate_history.len() > RATE_WINDOW {
                            rate_history.pop_front();
                        }
                    }
                    last_sample_time = now;
                    last_sample_bytes = total_bytes;
                }

                // 定期报告（每秒或每100个包）
                if now - last_report > Duration::from_secs(1) || received_packets % 10 == 0 {
                    let total_duration = now - start_time;
                    let avg_rate = (total_bytes as f64 * 8.0) / total_duration.as_secs_f64() / 1_000_000.0;
                    let window_avg = if !rate_history.is_empty() {
                        rate_history.iter().sum::<f64>() / rate_history.len() as f64
                    } else { 0.0 };

                    log::info!(
                        "Network Statistics:\n\
                         ├─ Current Rate: {:6.2} Mbps (Window Avg: {:6.2})\n\
                         ├─ Peak Rate:    {:6.2} Mbps\n\
                         ├─ Average Rate: {:6.2} Mbps\n\
                         ├─ Duration:     {:.2?}\n\
                         └─ Throughput:   {} packets, {} MB",
                        ((total_bytes - last_bytes) as f64 * 8.0) / (now - last_update).as_secs_f64() / 1_000_000.0,
                        window_avg,
                        peak_rate.load(Ordering::Relaxed) as f64 / 100.0,
                        avg_rate,
                        total_duration,
                        received_packets,
                        total_bytes / 1_000_000
                    );

                    last_report = now;
                }
                // ======================

                let now_sys = SystemTime::now();
                if let Err(e) = receiver.push(&endpoint, &buf[..n], now_sys) {
                    log::error!("Error processing packet: {:?}", e);
                }

                // 定期清理
                if received_packets % 1000 == 0 {
                    receiver.cleanup(now_sys);
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::TimedOut {
                    // 超时报告统计
                    let elapsed = start_time.elapsed();
                    log::info!(
                        "Receive timeout. Final Stats:\n\
                         ├─ Peak Rate:    {:6.2} Mbps\n\
                         ├─ Average Rate: {:6.2} Mbps\n\
                         ├─ Duration:     {:.2?}\n\
                         └─ Total:       {} packets, {} MB",
                        peak_rate.load(Ordering::Relaxed) as f64 / 100.0,
                        (total_bytes as f64 * 8.0) / elapsed.as_secs_f64() / 1_000_000.0,
                        elapsed,
                        received_packets,
                        total_bytes / 1_000_000
                    );
                    continue;
                }
                log::error!("Failed to receive data: {:?}", e);
            }
        }
    }
}