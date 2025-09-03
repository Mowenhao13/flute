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

// flute-sender/src/main.rs
use flute::{
    core::UDPEndpoint,
    core::lct::Cenc,
    core::Oti,
    sender::{ObjectDesc, Sender, Config as FluteConfig},
};
use std::{net::UdpSocket, time::SystemTime};
use serde::Deserialize;
use std::fs;

// 配置结构体
#[derive(Debug, Deserialize)]
struct AppConfig {
    network: NetworkConfig,
    fec: FecConfig,
    flute: FluteSettings,
    transfer: TransferConfig,
    logging: LoggingConfig,
    files: Vec<FileConfig>,
    advanced: AdvancedConfig,
}

#[derive(Debug, Deserialize)]
struct NetworkConfig {
    destination: String,
    bind_address: String,
    bind_port: u16,
    send_buffer_size: usize,
    send_interval_micros: u64,
}

#[derive(Debug, Deserialize)]
struct FecConfig {
    #[serde(rename = "type")]
    fec_type: String,
    symbol_size: u32,
    source_symbols: u32,
    repair_symbols: u32,
    encoding_symbol_id_length: u8,
    maximum_source_block_length: u8,
}

#[derive(Debug, Deserialize)]
struct FluteSettings {
    tsi: u32,
    content_type: String,
    enable_md5: bool,
    version: u32,
    publish_mode: String,
}

#[derive(Debug, Deserialize)]
struct TransferConfig {
    priority_queue: u8,
    interleave_blocks: u32,
    target_duration_secs: u64,
    carousel_mode: String,
}

#[derive(Debug, Deserialize)]
struct LoggingConfig {
    level: String,
    show_progress: bool,
    progress_interval: u32,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    path: String,
    content_type: String,
    priority: u8,
    version: u32,
}

#[derive(Debug, Deserialize)]
struct AdvancedConfig {
    cenc: String,
    use_complete_fdt: bool,
    allow_updates: bool,
}

fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let config_str = fs::read_to_string("sender_config.yaml")?;
    let config: AppConfig = serde_yaml::from_str(&config_str)?;
    Ok(config)
}

fn main() {
    let start_time = std::time::Instant::now(); 
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

    log::info!("Starting FLUTE sender with config: {:?}", config);

    // 创建FEC配置 - 修复类型转换
    let oti = match config.fec.fec_type.as_str() {
        "no_code" => Oti::new_no_code(
            config.fec.symbol_size.try_into().unwrap(),
            config.fec.source_symbols.try_into().unwrap()
        ),
        "reed_solomon" => Oti::new_reed_solomon_rs28(
            config.fec.symbol_size.try_into().unwrap(),
            config.fec.source_symbols.try_into().unwrap(),
            config.fec.repair_symbols.try_into().unwrap(),
        ).expect("Failed to create Reed-Solomon OTI"),
        "raptor" => Oti::new_raptor(
            config.fec.symbol_size.try_into().unwrap(),
            config.fec.source_symbols.try_into().unwrap(),
            config.fec.repair_symbols.try_into().unwrap(),
            config.fec.encoding_symbol_id_length.into(),
            config.fec.maximum_source_block_length.into(),
        ).expect("Failed to create Raptor OTI"),  // 添加 expect 处理 Result
        "raptorq" => Oti::new_raptorq(
            config.fec.symbol_size.try_into().unwrap(),
            config.fec.source_symbols.try_into().unwrap(),
            config.fec.repair_symbols.try_into().unwrap(),
            config.fec.encoding_symbol_id_length.into(),
            config.fec.maximum_source_block_length.into(),
        ).expect("Failed to create RaptorQ OTI"),  // 添加 expect 处理 Result
        _ => panic!("Unknown FEC type: {}", config.fec.fec_type),
    };

    // 创建FLUTE发送器配置 - 修复类型转换
    let mut flute_config = FluteConfig::default();
    flute_config.interleave_blocks = config.transfer.interleave_blocks.try_into().unwrap();

    // 创建 UDP 端点 - 使用 clone() 避免移动
    let endpoint = UDPEndpoint::new(None, config.network.bind_address.clone(), 3400);

    // 创建 UDP socket - 现在可以安全使用 config.network.bind_address
    let udp_socket = UdpSocket::bind(format!("{}:{}", config.network.bind_address, config.network.bind_port))
        .unwrap();
    
    // 移除不存在的set_send_buffer_size方法
    // udp_socket.set_send_buffer_size(config.network.send_buffer_size).unwrap();

    // 连接到目标
    udp_socket.connect(&config.network.destination).expect("Connection failed");

    // 创建FLUTE发送器 - 修复类型转换
    let mut sender = Sender::new(endpoint, config.flute.tsi.into(), &oti, &flute_config);

    log::info!("Sending to {}", config.network.destination);

    // 添加文件到发送器 - 修复类型转换
    for file_config in &config.files {
        let path = std::path::Path::new(&file_config.path);
        
        if !path.is_file() {
            log::error!("File not found: {}", file_config.path);
            continue;
        }

        log::info!("Inserting file: {}", file_config.path);
        
        let cenc = match config.advanced.cenc.as_str() {
            "Null" => Cenc::Null,
            "Deflate" => Cenc::Deflate,
            "Zlib" => Cenc::Zlib,
            "Gzip" => Cenc::Gzip,
            _ => Cenc::Null,
        };

        let obj = ObjectDesc::create_from_file(
            path,
            None,
            &file_config.content_type,
            config.flute.enable_md5,
            file_config.version,
            None,
            None,
            None,
            None,
            cenc,
            config.advanced.use_complete_fdt,
            None,
            config.advanced.allow_updates,
        ).unwrap();
        
        sender.add_object(config.transfer.priority_queue.into(), obj).unwrap();
    }

    log::info!("Publish FDT update");
    sender.publish(SystemTime::now()).unwrap();
    
    let mut sent_packets = 0;
    while let Some(pkt) = sender.read(SystemTime::now()) {
        match udp_socket.send(&pkt) {
            Ok(_) => {
                sent_packets += 1;
                if config.logging.show_progress && sent_packets % config.logging.progress_interval == 0 {
                    log::info!("Sent {} packets", sent_packets);
                }
            }
            Err(e) => {
                log::error!("Failed to send packet: {}", e);
            }
        }
        std::thread::sleep(std::time::Duration::from_micros(config.network.send_interval_micros));
    }

    let file_size = config.files.iter()
    .map(|f| std::fs::metadata(&f.path).map(|m| m.len()).unwrap_or(0))
    .sum::<u64>();

    let total_time = start_time.elapsed();
    let file_size = config.files.iter()
    .map(|f| std::fs::metadata(&f.path).map(|m| m.len()).unwrap_or(0))
    .sum::<u64>();

    let symbol_size = config.fec.symbol_size as u64;  // 转换为u64避免乘法类型冲突
    let total_packet_capacity = sent_packets as u64 * symbol_size;
    let overhead_percentage = if total_packet_capacity > 0 {
        (1.0 - (file_size as f64 / total_packet_capacity as f64)) * 100.0
    } else { 0.0 };

    let effective_rate = if total_time.as_secs_f64() > 0.0 {
        (file_size as f64 * 8.0) / total_time.as_secs_f64() / 1_000_000.0
    } else { 0.0 };

    log::info!(
        "\n=== Transfer Summary ===\n\
        Files:      {}\n\
        Size:       {:.2} MB\n\
        Duration:   {:.2?}\n\
        Rate:       {:.2} Mbps\n\
        Packets:    {}\n\
        Efficiency: {:.1}% (overhead: {:.1}%)\n\
        ========================",
        config.files.len(),
        file_size as f64 / 1_000_000.0,
        total_time,
        effective_rate,
        sent_packets,
        100.0 - overhead_percentage,
        overhead_percentage
    );
    log::info!("File transfer completed. Total packets sent: {}", sent_packets);
}
