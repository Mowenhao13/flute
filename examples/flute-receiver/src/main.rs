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


// use flute::{
//     core::UDPEndpoint,
//     receiver::{writer, Config as ReceiverConfig, MultiReceiver},
//     error::FluteError,
// };
// use serde::Deserialize;
// use std::fs;
// use std::path::Path;
// use std::rc::Rc;
// use std::time::{SystemTime, Duration};
//
// #[derive(Debug, Deserialize)]
// struct AppConfig {
//     receiver: ReceiverConfigSection,
// }
//
// #[derive(Debug, Deserialize)]
// struct ReceiverConfigSection {
//     network: ReceiverNetworkConfig,
//     storage: ReceiverStorageConfig,
//     logging: ReceiverLoggingConfig,
//     advanced: ReceiverAdvancedConfig,
// }
//
// #[derive(Debug, Deserialize)]
// struct ReceiverNetworkConfig {
//     bind_address: String,
//     port: u16,
// }
//
// #[derive(Debug, Deserialize)]
// struct ReceiverStorageConfig {
//     destination_dir: String,
//     enable_md5_check: bool,
// }
//
// #[derive(Debug, Deserialize)]
// struct ReceiverLoggingConfig {
//     progress_interval: u32,
// }
//
// #[derive(Debug, Deserialize)]
// struct ReceiverAdvancedConfig {
//     buffer_size: usize,
//     cleanup_interval: u32,
//     log_interval: u32,
//     max_memory_mb: u64,
//     max_retries: u32,
//     retry_delay_ms: u64,
//     #[serde(default = "default_true")]
//     keep_partial_files: bool,
// }
//
// fn default_true() -> bool {
//     true
// }
//
// fn load_config(config_path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
//     log::debug!("Loading configuration from: {}", config_path.display());
//     let config_str = fs::read_to_string(config_path)?;
//     let config: AppConfig = serde_yaml::from_str(&config_str)?;
//     Ok(config)
// }
//
// fn main() {
//     std::env::set_var("RUST_LOG", "info");
//     env_logger::builder().try_init().ok();
//
//     let config_path = Path::new("/home/halllo/flute-main/examples/config/config_1024mb_raptorq.yaml");
//     let config = match load_config(&config_path) {
//         Ok(cfg) => {
//             log::info!("Using configuration file: {}", config_path.display());
//             cfg
//         }
//         Err(e) => {
//             eprintln!(
//                 "Failed to load config from {}: {}",
//                 config_path.display(),
//                 e
//             );
//             std::process::exit(1);
//         }
//     };
//
//     let endpoint = UDPEndpoint::new(
//         None,
//         config.receiver.network.bind_address.clone(),
//         config.receiver.network.port,
//     );
//
//     let dest_dir = Path::new(&config.receiver.storage.destination_dir);
//     if !dest_dir.is_dir() {
//         if let Err(e) = std::fs::create_dir_all(dest_dir) {
//             log::error!("Failed to create directory {:?}: {}", dest_dir, e);
//             std::process::exit(-1);
//         }
//         log::info!("Created destination directory: {:?}", dest_dir);
//     }
//
//     log::info!("Create FLUTE receiver, writing objects to {:?}", dest_dir);
//
//     let mut receiver_config = ReceiverConfig::default();
//     receiver_config.object_max_cache_size = Some(config.receiver.advanced.max_memory_mb as usize * 1024 * 1024);
//
//     let writer = match writer::ObjectWriterFSBuilder::new(dest_dir, config.receiver.storage.enable_md5_check) {
//         Ok(builder) => Rc::new(builder),
//         Err(e) => {
//             log::error!("Failed to create writer: {:?}", e);
//             std::process::exit(1);
//         }
//     };
//
//     let mut receiver = MultiReceiver::new(writer, Some(receiver_config), false);
//
//     let socket = match std::net::UdpSocket::bind(format!(
//         "{}:{}",
//         config.receiver.network.bind_address, config.receiver.network.port
//     )) {
//         Ok(socket) => socket,
//         Err(e) => {
//             log::error!("Failed to bind UDP socket: {}", e);
//             std::process::exit(1);
//         }
//     };
//
//     log::info!(
//         "Listening on port {} for FLUTE data",
//         config.receiver.network.port
//     );
//
//     let mut buf = vec![0; config.receiver.advanced.buffer_size];
//     let mut received_packets = 0;
//     let max_memory_bytes = config.receiver.advanced.max_memory_mb * 1024 * 1024;
//     let mut memory_usage: u64 = 0;
//
//     let max_retries = config.receiver.advanced.max_retries;
//     let retry_delay = Duration::from_millis(config.receiver.advanced.retry_delay_ms);
//     let mut retry_count = 0;
//
//
//     loop {
//         match socket.recv_from(&mut buf) {
//             Ok((n, src)) => {
//                 retry_count = 0;
//                 received_packets += 1;
//                 memory_usage += n as u64;
//
//                 if memory_usage > max_memory_bytes {
//                     log::warn!(
//                         "Memory usage {}MB exceeds limit {}MB, forcing cleanup",
//                         memory_usage / (1024 * 1024),
//                         config.receiver.advanced.max_memory_mb
//                     );
//                     let now = SystemTime::now();
//                     receiver.cleanup(now);
//                     memory_usage = 0;
//                 }
//
//                 if received_packets % config.receiver.advanced.log_interval == 0 {
//                     log::info!("Received {} packets from {}", received_packets, src);
//                 }
//
//                 let now = SystemTime::now();
//                 if let Err(e) = receiver.push(&endpoint, &buf[..n], now) {
//                     log::warn!("Packet processing error (will retry): {:?}", e);
//                     std::thread::sleep(retry_delay);
//                     continue;
//                 }
//
//                 if config.receiver.advanced.cleanup_interval > 0 &&
//                    received_packets % config.receiver.advanced.cleanup_interval == 0
//                 {
//                     let now = SystemTime::now();
//                     receiver.cleanup(now);
//                     memory_usage = 0;
//                 }
//             }
//             Err(e) => {
//                 retry_count += 1;
//                 if retry_count >= max_retries {
//                     log::error!("Max retries ({}) reached: {}", max_retries, e);
//                     break;
//                 }
//                 log::warn!(
//                     "Receive error (retry {}/{}): {}",
//                     retry_count,
//                     max_retries,
//                     e
//                 );
//                 std::thread::sleep(retry_delay);
//             }
//         }
//     }
// }
//
use flute::{
    core::UDPEndpoint,
    receiver::{writer, Config as ReceiverConfig, MultiReceiver},
    error::FluteError,
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::time::{SystemTime, Duration};

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
    std::env::set_var("RUST_LOG", "info");
    env_logger::builder().try_init().ok();

    let config_path = Path::new("/home/Halllo/Projects/flute/examples/config/config_1024mb_raptorq.yaml");
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

    log::info!(
        "Listening on port {} for FLUTE data",
        config.receiver.network.port
    );

    let mut buf = vec![0; config.receiver.advanced.buffer_size];
    let mut received_packets = 0;
    let max_memory_bytes = config.receiver.advanced.max_memory_mb * 1024 * 1024;
    let mut memory_usage: u64 = 0;

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, src)) => {
                received_packets += 1;
                memory_usage += n as u64;

                if memory_usage > max_memory_bytes {
                    log::warn!(
                        "Memory usage {}MB exceeds limit {}MB, forcing cleanup",
                        memory_usage / (1024 * 1024),
                        config.receiver.advanced.max_memory_mb
                    );
                    let now = SystemTime::now();
                    receiver.cleanup(now);
                    memory_usage = 0;
                }

                if received_packets % config.receiver.advanced.log_interval == 0 {
                    log::info!("Received {} packets from {}", received_packets, src);
                }

                let now = SystemTime::now();
                if let Err(e) = receiver.push(&endpoint, &buf[..n], now) {
                    log::warn!("Packet processing error: {:?}", e);
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