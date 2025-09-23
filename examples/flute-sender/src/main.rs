// use flute::{
//     core::UDPEndpoint,
//     core::lct::Cenc,
//     sender::{ObjectDesc, Sender},
// };
// use std::{net::UdpSocket, time::SystemTime};

// fn main() {
//     std::env::set_var("RUST_LOG", "info");
//     env_logger::builder().try_init().ok();

//     // 从命令行参数获取目标地址
//     let args: Vec<String> = std::env::args().collect();
//     if args.len() < 3 {
//         println!("Send files over UDP/FLUTE using unicast");
//         println!("Usage: {} destination_ip:port path/to/file1 path/to/file2 ...", args[0]);
//         println!("Example: {} 172.18.202.15:3400 file1.txt file2.jpg", args[0]);
//         std::process::exit(0);
//     }

//     let dest = &args[1];

//     // 使用单播地址而不是组播地址
//     let endpoint = UDPEndpoint::new(None, "0.0.0.0".to_owned(), 3400);

//     log::info!("Create UDP Socket");

//     // 绑定到所有接口
//     let udp_socket = UdpSocket::bind("0.0.0.0:0").unwrap();

//     // 设置发送缓冲区大小
//     // udp_socket.set_send_buffer_size(1024 * 1024).unwrap();

//     log::info!("Create FLUTE Sender");
//     let tsi = 1;
//     let mut sender = Sender::new(endpoint, tsi, &Default::default(), &Default::default());

//     log::info!("Sending to {}", dest);
//     udp_socket.connect(dest).expect("Connection failed");

//     for file in &args[2..] {
//         let path = std::path::Path::new(file);

//         if !path.is_file() {
//             log::error!("{} is not a file", file);
//             continue; // 跳过无效文件而不是退出
//         }

//         log::info!("Insert file {} to FLUTE sender", file);
//         let obj = ObjectDesc::create_from_file(
//             path,
//             None,
//             "application/octet-stream",
//             true,
//             1,
//             None,
//             None,
//             None,
//             None,
//             Cenc::Null,
//             true,
//             None,
//             true,
//         )
//             .unwrap();
//         sender.add_object(0, obj).unwrap();
//     }

//     log::info!("Publish FDT update");
//     sender.publish(SystemTime::now()).unwrap();

//     let mut sent_packets = 0;
//     while let Some(pkt) = sender.read(SystemTime::now()) {
//         match udp_socket.send(&pkt) {
//             Ok(_) => {
//                 sent_packets += 1;
//                 if sent_packets % 100 == 0 {
//                     log::info!("Sent {} packets", sent_packets);
//                 }
//             }
//             Err(e) => {
//                 log::error!("Failed to send packet: {}", e);
//             }
//         }
//         // 稍微减慢发送速度以避免拥塞
//         std::thread::sleep(std::time::Duration::from_micros(10));
//     }

//     log::info!("File transfer completed. Total packets sent: {}", sent_packets);
// }

// use flute::{
//     core::lct::Cenc,
//     core::Oti,
//     core::UDPEndpoint,
//     sender::{Config as SenderConfig, ObjectDesc, Sender},
// };
// use serde::Deserialize;
// use std::fs;
// use std::path::Path;
// use std::time::{Duration, Instant};
// use std::{net::UdpSocket, time::SystemTime};
//
// use rayon::prelude::*; // 添加rayon依赖
// use crossbeam_channel::{bounded, Receiver, Sender as CrossbeamSender}; // 添加crossbeam-channel依赖
// use std::sync::{Arc, Mutex};
// use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
//
// #[derive(Debug, Deserialize)]
// struct AppConfig {
//     sender: SenderConfigSection,
// }
//
// #[derive(Debug, Deserialize)]
// struct SenderConfigSection {
//     network: SenderNetworkConfig,
//     fec: SenderFecConfig,
//     flute: SenderFluteConfig,
//     logging: SenderLoggingConfig,
//     files: Vec<FileConfig>,
//     // New param
//     max_rate_kbps: Option<u32>,        // 最大速率限制 (kbps)
//     send_interval_micros: Option<u64>, // 发送间隔微秒
// }
//
// #[derive(Debug, Deserialize)]
// struct SenderNetworkConfig {
//     destination: String,
//     bind_address: String,
//     bind_port: u16,
//     send_interval_micros: u64,
// }
//
// #[derive(Debug, Deserialize)]
// struct SenderFecConfig {
//     #[serde(rename = "type")]
//     fec_type: String,
//     encoding_symbol_length: u16,
//     max_number_of_parity_symbols: u32,
//     encoding_symbol_id_length: u8,
//     maximum_source_block_length: u32,
//     symbol_alignment: u8,
//     sub_blocks_length: u16,
// }
//
// #[derive(Debug, Deserialize)]
// struct SenderFluteConfig {
//     tsi: u32,
//     interleave_blocks: u32,
// }
//
// #[derive(Debug, Deserialize)]
// struct SenderLoggingConfig {
//     progress_interval: u32,
// }
//
// #[derive(Debug, Deserialize)]
// struct FileConfig {
//     path: String,
//     content_type: String,
//     priority: u8,
//     version: u32,
// }
//
// struct PerformanceStats {
//     total_bytes_sent: AtomicU64,
//     total_packets_sent: AtomicUsize,
//     start_time: Instant,
// }
//
// impl PerformanceStats {
//     fn new() -> Self {
//         Self {
//             total_bytes_sent: AtomicU64::new(0),
//             total_packets_sent: AtomicUsize::new(0),
//             start_time: Instant::now(),
//         }
//     }
//
//     fn add_bytes(&self, bytes: usize) {
//         self.total_bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
//         self.total_packets_sent.fetch_add(1, Ordering::Relaxed);
//     }
//
//     fn get_stats(&self) -> (u64, usize, f64) {
//         let bytes = self.total_bytes_sent.load(Ordering::Relaxed);
//         let packets = self.total_packets_sent.load(Ordering::Relaxed);
//         let elapsed = self.start_time.elapsed().as_secs_f64();
//         (bytes, packets, elapsed)
//     }
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
//     // 确认CPU核心数
//     let available_parallelism = std::thread::available_parallelism().unwrap().get();
//     log::info!("Available CPU cores: {}", available_parallelism);
//
//     let config_path =
//         Path::new("/home/Halllo/Projects/flute/examples/config/config_1024mb_raptorq.yaml");
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
//     // 计算所有文件的总原始大小
//     let total_file_size: usize = config.sender.files.iter()
//         .map(|f| {
//             fs::metadata(&f.path)
//                 .map(|m| m.len() as usize)
//                 .unwrap_or(0)
//         })
//         .sum();
//
//     log::info!("Total file size to transmit: {} bytes ({} MB)",
//     total_file_size,
//     total_file_size as f64 / (1024.0 * 1024.0));
//
//     let endpoint = UDPEndpoint::new(
//         None,
//         config.sender.network.bind_address.clone(),
//         config.sender.network.bind_port,
//     );
//
//
//     let udp_socket = UdpSocket::bind(format!(
//         "{}:{}",
//         config.sender.network.bind_address, config.sender.network.bind_port
//     ))
//     .unwrap();
//
//     let tsi = config.sender.flute.tsi;
//
//     // 创建OTI配置
//     // let symbol_size: u16 = config.sender.fec.symbol_size.try_into().unwrap();
//     // let source_symbols: u16 = config.sender.fec.source_symbols.try_into().unwrap();
//     let max_number_of_parity_symbols: u16 = config.sender.fec.max_number_of_parity_symbols.try_into().unwrap();
//     let encoding_symbol_length: u16 = config.sender.fec.encoding_symbol_length.try_into().unwrap();
//     // let source_symbols: u16 = config.sender.fec.source_symbols.try_into().unwrap();
//     let encoding_symbol_id_length = config.sender.fec.encoding_symbol_id_length;
//     let max_source_block_length = config.sender.fec.maximum_source_block_length;
//     let symbol_alignment = config.sender.fec.symbol_alignment;
//     let sub_blocks_length = config.sender.fec.sub_blocks_length;
//
//     let oti = match config.sender.fec.fec_type.as_str() {
//         "no_code" => Oti::new_no_code(encoding_symbol_length, max_source_block_length),
//         "reed_solomon_gf28" => Oti::new_reed_solomon_rs28(
//             encoding_symbol_length,
//             max_source_block_length,
//             max_number_of_parity_symbols as u8,
//         ).expect("Invalid Reed Solomon GF28 parameters"),
//         "reed_solomon_gf28_under_specified" => Oti::new_reed_solomon_rs28_under_specified(
//             encoding_symbol_length,
//             max_source_block_length,
//             max_number_of_parity_symbols,
//         ).expect("Invalid Reed Solomon GF28 Under Specified parameters"),
//         "raptor" => Oti::new_raptor(
//             encoding_symbol_length,
//             max_source_block_length,
//             max_number_of_parity_symbols,
//             encoding_symbol_id_length,
//             symbol_alignment, // 默认对齐参数
//         ).expect("Invalid Raptor parameters"),
//         "raptorq" => Oti::new_raptorq(
//             encoding_symbol_length,
//             max_source_block_length,
//             max_number_of_parity_symbols,
//             sub_blocks_length,
//             symbol_alignment,
//         ).expect("Invalid RaptorQ parameters"),
//         _ => panic!("Unsupported FEC type: {}", config.sender.fec.fec_type),
//     };
//
//     log::info!("Using FEC: {:?}", oti.fec_encoding_id);
//     log::info!("Symbol size: {} bytes", oti.encoding_symbol_length);
//     log::info!("Max source block length: {}", oti.maximum_source_block_length);
//     log::info!("Max parity symbols: {}", oti.max_number_of_parity_symbols);
//     log::info!("Max symbol alignment: {}", oti.max_number_of_parity_symbols);
//     let mut sender_config = SenderConfig::default();
//     sender_config.interleave_blocks = config.sender.flute.interleave_blocks.try_into().unwrap();
//
//     let mut sender = Sender::new(endpoint, tsi.into(), &oti, &sender_config);
//
//     udp_socket
//         .connect(&config.sender.network.destination)
//         .unwrap();
//
//
//     for file_config in &config.sender.files {
//         let path = Path::new(&file_config.path);
//         if !path.is_file() {
//             log::error!("File not found: {}", file_config.path);
//             continue;
//         }
//
//         log::info!("Insert file {} to FLUTE sender", file_config.path);
//         let obj = ObjectDesc::create_from_file(
//             path,
//             None,
//             &file_config.content_type,
//             true,
//             file_config.version,
//             None,
//             None,
//             None,
//             None,
//             Cenc::Null,
//             true,
//             None,
//             true,
//         )
//         .unwrap();
//         sender.add_object(file_config.priority.into(), obj).unwrap();
//     }
//
//     sender.publish(SystemTime::now()).unwrap();
//
//     // 创建并行处理通道
//     let (packet_tx, packet_rx): (CrossbeamSender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(1024 * available_parallelism);
//     let sender = Arc::new(Mutex::new(sender));
//     // let udp_socket = Arc::new(Mutex::new(udp_socket));
//     let stats = Arc::new(PerformanceStats::new());
//
//     // 创建编码线程池
//     let encoding_pool = rayon::ThreadPoolBuilder::new()
//         .num_threads(available_parallelism)
//         .build()
//         .unwrap();
//
//     // 启动编码线程
//     let sender_mutex_clone = sender.clone();
//     let socket = Arc::new(Mutex::new(udp_socket));
//     let stats_clone = stats.clone();
//
//     for i in 0..available_parallelism {
//         let socket = socket.clone();
//         let packet_rx = packet_rx.clone();
//         let stats = stats_clone.clone();
//         std::thread::spawn(move || {
//             log::info!("Send thread {} started", i);
//             let mut batch = Vec::with_capacity(32);
//
//             while let Ok(packet) = packet_rx.recv() {
//                 batch.push(packet);
//
//                 // 批量发送优化
//                 if batch.len() >= 32 {
//                     for pkt in batch.drain(..) {
//                         match socket.lock().unwrap().send(&pkt) {
//                             Ok(bytes_sent) => {
//                                 stats.add_bytes(bytes_sent);
//                             }
//                             Err(e) => {
//                                 log::error!("Send error in thread {}: {}", i, e);
//                             }
//                         }
//                     }
//                 }
//             }
//         });
//     }
//
//
//     // 网络发送线程
//     let start_time = Instant::now();
//     let mut total_bytes_sent = 0;
//     let mut sent_packets = 0;
//     let mut last_log_time = Instant::now();
//     let mut bytes_sent_since_last_log = 0;
//
//
//
//     // 使用 Rayon 的线程池
//     let pool = rayon::ThreadPoolBuilder::new()
//         .num_threads(available_parallelism)
//         .build()
//         .unwrap();
//
//     let mut packets_generated = 0;
//     loop {
//         let packet = {
//             let mut sender = sender.lock().unwrap();
//             sender.read(SystemTime::now())
//         };
//
//         match packet {
//             Some(packet) => {
//                 packets_generated += 1;
//
//                 // 使用线程池进行并行处理
//                 let packet_tx = packet_tx.clone();
//                 pool.spawn(move || {
//                     // 直接发送数据包，不进行额外处理
//                     if let Err(e) = packet_tx.send(packet) {
//                         log::error!("Failed to send packet to channel: {}", e);
//                     }
//                 });
//
//                 // 进度日志
//                 if packets_generated % config.sender.logging.progress_interval == 0 {
//                     let elapsed = last_log_time.elapsed().as_secs_f64();
//                     let (total_bytes, total_packets, total_elapsed) = stats.get_stats();
//
//                     if total_elapsed > 0.0 {
//                         log::info!(
//                             "Progress: {} packets generated, {} sent, {:.2} MB sent, {:.2} Mbps",
//                             packets_generated,
//                             total_packets,
//                             total_bytes as f64 / (1024.0 * 1024.0),
//                             (total_bytes as f64 * 8.0) / total_elapsed / 1_000_000.0
//                         );
//                     } else {
//                         log::info!(
//                             "Progress: {} packets generated, {} sent, {:.2} MB sent",
//                             packets_generated,
//                             total_packets,
//                             total_bytes as f64 / (1024.0 * 1024.0)
//                         );
//                     }
//
//                     last_log_time = Instant::now();
//                 }
//             }
//             None => {
//                 log::info!("No more packets to generate");
//                 break;
//             }
//         }
//     }
//
//     // 传输完成后的详细统计
//     let total_time = start_time.elapsed();
//     let total_mb_sent = total_bytes_sent as f64 / (1024.0 * 1024.0);
//     let total_mb_recv = total_file_size as f64 / (1024.0 * 1024.0);
//
//     // 总共传输文件大小除以用时
//     let average_rate_mbps_sender = (total_bytes_sent as f64 * 8.0) / total_time.as_secs_f64() / 1_000_000.0;
//     // 原始文件大小除以用时
//     let average_rate_mbps_receiver = (total_file_size as f64 * 8.0) / total_time.as_secs_f64() / 1_000_000.0;
//
//     log::info!("==========================================");
//     log::info!("FILE TRANSFER COMPLETED");
//     log::info!("==========================================");
//     log::info!("Total time: {:.2} seconds", total_time.as_secs_f64());
//     log::info!("Total packets: {}", sent_packets);
//     log::info!("Total data sent: {:.2} MB", total_mb_sent);
//     log::info!("Total data received: {:.2} MB", total_mb_recv);
//     log::info!("Average rate for sender: {:.2} Mbps", average_rate_mbps_sender);
//     log::info!("Average rate for sender: {:.2} MB/s", average_rate_mbps_sender / 8.0);
//     log::info!("Average rate for receiver: {:.2} Mbps", average_rate_mbps_receiver);
//     log::info!("Average rate for receiver: {:.2} MB/s", average_rate_mbps_receiver / 8.0);
//     log::info!("Packet rate: {:.2} packets/second",
//                sent_packets as f64 / total_time.as_secs_f64());
//     log::info!("==========================================");
//     log::info!(
//         "File transfer completed. Total packets sent: {}",
//         sent_packets
//     );
// }


use flute::{
    core::lct::Cenc,
    core::Oti,
    core::UDPEndpoint,
    sender::{Config as SenderConfig, ObjectDesc, Sender},
};
use serde::Deserialize;
use std::fs;
use std::path::Path;
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
    send_interval_micros: Option<u64>, // 发送间隔微秒
}

#[derive(Debug, Deserialize)]
struct SenderNetworkConfig {
    destination: String,
    bind_address: String,
    bind_port: u16,
    send_interval_micros: u64,
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

fn load_config(config_path: &Path) -> Result<AppConfig, Box<dyn std::error::Error>> {
    log::debug!("Loading configuration from: {}", config_path.display());
    let config_str = fs::read_to_string(config_path)?;
    let config: AppConfig = serde_yaml::from_str(&config_str)?;
    Ok(config)
}

fn main() {

    std::env::set_var("RUST_LOG", "info");
    env_logger::builder().try_init().ok();

    let choice = 5;  // 这里手动改数字就行

    // 配置文件列表
    let configs = vec![
        "/home/Halllo/Projects/flute/examples/config/config_1mb_no_code.yaml",
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_no_code.yaml",
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_raptor.yaml",
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_raptorq.yaml",
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_reed_solomon_rs28.yaml",
        "/home/Halllo/Projects/flute/examples/config/config_1024mb_reed_solomon_rs28_under_specified.yaml",
    ];

    if choice < 1 || choice > configs.len() {
        eprintln!("Invalid choice {}, must be 1..{}", choice, configs.len());
        std::process::exit(1);
    }

    let config_path = Path::new(configs[choice - 1]);

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

    // 创建OTI配置
    // let symbol_size: u16 = config.sender.fec.symbol_size.try_into().unwrap();
    // let source_symbols: u16 = config.sender.fec.source_symbols.try_into().unwrap();
    let max_number_of_parity_symbols: u16 = config.sender.fec.max_number_of_parity_symbols.try_into().unwrap();
    let encoding_symbol_length: u16 = config.sender.fec.encoding_symbol_length.try_into().unwrap();
    // let source_symbols: u16 = config.sender.fec.source_symbols.try_into().unwrap();
    // let encoding_symbol_id_length = config.sender.fec.encoding_symbol_id_length;
    let max_source_block_length = config.sender.fec.maximum_source_block_length;
    let symbol_alignment = config.sender.fec.symbol_alignment;
    let sub_blocks_length = config.sender.fec.sub_blocks_length;

    let oti = match config.sender.fec.fec_type.as_str() {
        "no_code" => Oti::new_no_code(encoding_symbol_length, max_source_block_length),
        "reed_solomon_gf28" => Oti::new_reed_solomon_rs28(
            encoding_symbol_length,
            max_source_block_length,
            max_number_of_parity_symbols as u8,
        ).expect("Invalid Reed Solomon GF28 parameters"),
        "reed_solomon_gf28_under_specified" => Oti::new_reed_solomon_rs28_under_specified(
            encoding_symbol_length,
            max_source_block_length,
            max_number_of_parity_symbols,
        ).expect("Invalid Reed Solomon GF28 Under Specified parameters"),
        "raptor" => Oti::new_raptor(
            encoding_symbol_length,
            max_source_block_length,
            max_number_of_parity_symbols,
            sub_blocks_length as u8,
            symbol_alignment, // 默认对齐参数
        ).expect("Invalid Raptor parameters"),
        "raptorq" => Oti::new_raptorq(
            encoding_symbol_length,
            max_source_block_length,
            max_number_of_parity_symbols,
            sub_blocks_length,
            symbol_alignment,
        ).expect("Invalid RaptorQ parameters"),
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

        log::info!("Insert file {} to FLUTE sender", file_config.path);
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

    let max_rate_kbps = config.sender.max_rate_kbps.unwrap_or(0);
    let bytes_per_sec = if max_rate_kbps > 0 {
        (max_rate_kbps as f64 * 1000.0 / 8.0) // kbps -> Bps
    } else {
        f64::INFINITY // 不限速
    };

    log::info!("Rate control: max_rate_kbps = {} ({} B/s)",
           max_rate_kbps, if bytes_per_sec.is_finite() { bytes_per_sec as u64 } else { 0 });

    // 用“下一次应发送时间”做节拍
    let mut next_send_at = Instant::now();

    // 日志辅助
    let mut last_log_time = Instant::now();
    let mut bytes_since_log: u64 = 0;

    while let Some(pkt) = sender.read(SystemTime::now()) {
        // 仅在限速开启时进行节拍控制
        if bytes_per_sec.is_finite() {
            let pkt_len = pkt.len() as f64;

            // 这一个包在目标速率下“应当占用”的时间片
            let interval = Duration::from_secs_f64(pkt_len / bytes_per_sec);

            // 若当前时间尚未到达下一次发送时刻，则等待
            let now = Instant::now();
            if now < next_send_at {
                std::thread::sleep(next_send_at - now);
            }

            // 发送成功后，推进下一次发送时刻
            next_send_at += interval;

            // 若由于调度/日志等原因漂移过大，进行重校准，避免越走越偏
            let drift = Instant::now().saturating_duration_since(next_send_at);
            if drift > Duration::from_millis(200) {
                next_send_at = Instant::now() + interval;
            }
        }

        match udp_socket.send(&pkt) {
            Ok(bytes_sent) => {
                total_bytes_sent += bytes_sent as u64;
                bytes_since_log += bytes_sent as u64;
                sent_packets += 1;

                // 按进度间隔打印统计
                if sent_packets % config.sender.logging.progress_interval as u64 == 0 {
                    let now = Instant::now();
                    let dt = now.duration_since(last_log_time).as_secs_f64();
                    if dt > 0.0 {
                        let inst_mbps = (bytes_since_log as f64 * 8.0) / dt / 1_000_000.0;
                        let avg_mbps = (total_bytes_sent as f64 * 8.0)
                            / now.duration_since(start_time).as_secs_f64() / 1_000_000.0;

                        log::info!(
                        "Progress: {} pkts, {} MB | Instant: {:.2} Mbps | Avg: {:.2} Mbps",
                        sent_packets,
                        total_bytes_sent / (1024 * 1024),
                        inst_mbps,
                        avg_mbps
                    );
                    }
                    last_log_time = now;
                    bytes_since_log = 0;
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

