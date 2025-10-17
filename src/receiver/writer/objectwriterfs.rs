use super::{ObjectMetadata, ObjectWriter, ObjectWriterBuilder, ObjectWriterBuilderResult};
use crate::{
    common::udpendpoint::UDPEndpoint,
    error::{FluteError, Result},
};
use std::{cell::RefCell, io::Write, time::{Duration, SystemTime}};

///
/// Write objects received by the `receiver` to a filesystem
///
#[derive(Debug)]
pub struct ObjectWriterFSBuilder {
    dest: std::path::PathBuf,
    enable_md5_check: bool,
}

impl ObjectWriterFSBuilder {
    /// Return a new `ObjectWriterBuffer`
    pub fn new(dest: &std::path::Path, enable_md5_check: bool) -> Result<ObjectWriterFSBuilder> {
        if !dest.is_dir() {
            return Err(FluteError::new(format!("{:?} is not a directory", dest)));
        }

        Ok(ObjectWriterFSBuilder {
            dest: dest.to_path_buf(),
            enable_md5_check,
        })
    }
}

impl ObjectWriterBuilder for ObjectWriterFSBuilder {
    fn new_object_writer(
        &self,
        _endpoint: &UDPEndpoint,
        _tsi: &u64,
        _toi: &u128,
        meta: &ObjectMetadata,
        _now: std::time::SystemTime,
    ) -> ObjectWriterBuilderResult {
        ObjectWriterBuilderResult::StoreObject(Box::new(ObjectWriterFS {
            dest: self.dest.clone(),
            inner: RefCell::new(ObjectWriterFSInner {
                destination: None,
                writer: None,
                bytes_written: 0,
                write_count: 0,
                transfer_start_time: None,
                last_write_time: None,
            }),
            meta: meta.clone(),
            enable_md5_check: self.enable_md5_check,
        }))
    }

    fn update_cache_control(
        &self,
        _endpoint: &UDPEndpoint,
        _tsi: &u64,
        _toi: &u128,
        _meta: &ObjectMetadata,
        _now: std::time::SystemTime,
    ) {
    }

    fn fdt_received(
        &self,
        _endpoint: &UDPEndpoint,
        _tsi: &u64,
        _fdt_xml: &str,
        _expires: std::time::SystemTime,
        _meta: &ObjectMetadata,
        _transfer_duration: std::time::Duration,
        _now: std::time::SystemTime,
        _ext_time: Option<std::time::SystemTime>,
    ) {
    }
}

///
/// Write an object to a file system.  
/// Uses the content-location to create the destination path of the object.  
/// If the destination path does not exists, the folder hierarchy is created.  
/// Existing files will be overwritten by this object.
///
#[derive(Debug)]
pub struct ObjectWriterFS {
    /// Folder destination were the object will be written
    dest: std::path::PathBuf,
    inner: RefCell<ObjectWriterFSInner>,
    meta: ObjectMetadata,
    enable_md5_check: bool,
}

///
///
#[derive(Debug)]
pub struct ObjectWriterFSInner {
    destination: Option<std::path::PathBuf>,
    writer: Option<std::io::BufWriter<std::fs::File>>,
    // 统计字段 - 用于问题诊断
    bytes_written: u64,
    write_count: u64,
    transfer_start_time: Option<SystemTime>,
    last_write_time: Option<SystemTime>,
}

impl ObjectWriter for ObjectWriterFS {
    fn open(&self, now: SystemTime) -> Result<()> {
        let url = url::Url::parse(&self.meta.content_location);
        let content_location_path = match &url {
            Ok(url) => url.path(),
            Err(e) => match e {
                url::ParseError::RelativeUrlWithoutBase => &self.meta.content_location,
                url::ParseError::RelativeUrlWithCannotBeABaseBase => &self.meta.content_location,
                _ => {
                    log::error!(
                        "Fail to parse content location {:?} {:?}",
                        self.meta.content_location,
                        e
                    );
                    return Err(FluteError::new(format!(
                        "Fail to parse content location {:?} {:?}",
                        self.meta.content_location, e
                    )));
                }
            },
        };
        let relative_path = content_location_path
            .strip_prefix('/')
            .unwrap_or(content_location_path);
        let destination = self.dest.join(relative_path);
        log::info!(
            "🚀 [RECV] 开始接收文件 {:?} -> {:?}",
            relative_path,
            destination
        );
        log::info!(
            "   预期大小: {} bytes ({:.2} MB)",
            self.meta.content_length.unwrap_or(0),
            self.meta.content_length.unwrap_or(0) as f64 / 1_048_576.0
        );
        
        let parent = destination.parent();
        if parent.is_some() {
            let parent = parent.unwrap();
            if !parent.is_dir() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let file = std::fs::File::create(&destination)?;
        let mut inner = self.inner.borrow_mut();
        inner.writer = Some(std::io::BufWriter::new(file));
        inner.destination = Some(destination.to_path_buf());
        inner.transfer_start_time = Some(now);
        inner.bytes_written = 0;
        inner.write_count = 0;
        inner.last_write_time = Some(now);
        Ok(())
    }

    fn write(&self, sbn: u32, data: &[u8], now: SystemTime) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        if inner.writer.is_none() {
            return Ok(());
        }
        
        let data_len = data.len();
        inner
            .writer
            .as_mut()
            .unwrap()
            .write_all(data)
            .map_err(|e| {
                log::error!("❌ [RECV] 写入失败 {:?} SBN={} 错误={:?}", inner.destination, sbn, e);
                FluteError::new(format!(
                    "Fail to write data to file {:?} {:?}",
                    inner.destination, e
                ))
            })?;
        
        // 更新统计
        inner.bytes_written += data_len as u64;
        inner.write_count += 1;
        inner.last_write_time = Some(now);
        
        // 定期输出进度（每100次写入）
        if inner.write_count % 100 == 0 {
            let expected_size = self.meta.content_length.unwrap_or(0);
            let progress = if expected_size > 0 {
                (inner.bytes_written as f64 / expected_size as f64) * 100.0
            } else {
                0.0
            };
            
            let duration = if let Some(start) = inner.transfer_start_time {
                now.duration_since(start).unwrap_or(Duration::from_secs(0))
            } else {
                Duration::from_secs(0)
            };
            
            let rate_mbps = if duration.as_secs_f64() > 0.0 {
                (inner.bytes_written as f64 * 8.0) / duration.as_secs_f64() / 1_000_000.0
            } else {
                0.0
            };
            
            log::info!(
                "📊 [RECV] 进度: {:.1}% ({}/{} MB) | 速率: {:.2} Mbps | SBN: {}",
                progress,
                inner.bytes_written as f64 / 1_048_576.0,
                expected_size as f64 / 1_048_576.0,
                rate_mbps,
                sbn
            );
        }
        
        Ok(())
    }

    fn complete(&self, now: SystemTime) {
        let mut inner = self.inner.borrow_mut();
        if inner.writer.is_none() {
            return;
        }

        // 计算传输统计
        let duration = if let Some(start) = inner.transfer_start_time {
            now.duration_since(start).unwrap_or(Duration::from_secs(0))
        } else {
            Duration::from_secs(0)
        };
        
        let rate_mbps = if duration.as_secs_f64() > 0.0 {
            (inner.bytes_written as f64 * 8.0) / duration.as_secs_f64() / 1_000_000.0
        } else {
            0.0
        };

        log::info!("✅ [RECV] 文件接收完成 {:?}", inner.destination);
        log::info!("   总字节数: {} ({:.2} MB)", inner.bytes_written, inner.bytes_written as f64 / 1_048_576.0);
        log::info!("   写入次数: {}", inner.write_count);
        log::info!("   传输时长: {:.2}s", duration.as_secs_f64());
        log::info!("   平均速率: {:.2} Mbps", rate_mbps);
        
        inner.writer.as_mut().unwrap().flush().ok();
        inner.writer = None;
        inner.destination = None
    }

    fn error(&self, now: SystemTime) {
        let mut inner = self.inner.borrow_mut();
        inner.writer = None;
        if inner.destination.is_some() {
            // 计算失败时的统计信息
            let expected_size = self.meta.content_length.unwrap_or(0);
            let progress = if expected_size > 0 {
                (inner.bytes_written as f64 / expected_size as f64) * 100.0
            } else {
                0.0
            };
            
            let duration = if let Some(start) = inner.transfer_start_time {
                now.duration_since(start).unwrap_or(Duration::from_secs(0))
            } else {
                Duration::from_secs(0)
            };
            
            let rate_mbps = if duration.as_secs_f64() > 0.0 {
                (inner.bytes_written as f64 * 8.0) / duration.as_secs_f64() / 1_000_000.0
            } else {
                0.0
            };
            
            log::error!("❌ ==================== 传输失败详细信息 ====================");
            log::error!("📁 文件路径: {:?}", inner.destination);
            log::error!("📊 传输统计:");
            log::error!("   - 已接收字节数: {} bytes ({:.2} MB)", inner.bytes_written, inner.bytes_written as f64 / 1_048_576.0);
            log::error!("   - 预期文件大小: {} bytes ({:.2} MB)", expected_size, expected_size as f64 / 1_048_576.0);
            log::error!("   - 完成百分比: {:.2}%", progress);
            log::error!("   - 写入次数: {}", inner.write_count);
            log::error!("⏱️  时间统计:");
            log::error!("   - 传输持续时间: {:.2}s", duration.as_secs_f64());
            log::error!("   - 平均接收速率: {:.2} Mbps", rate_mbps);
            if let Some(last_write) = inner.last_write_time {
                if let Ok(idle) = now.duration_since(last_write) {
                    log::error!("   - 最后写入距今: {:.2}s (可能超时)", idle.as_secs_f64());
                }
            }
            log::error!("🔧 可能原因:");
            if progress < 10.0 {
                log::error!("   - 传输刚开始就失败，可能是网络连接问题或FEC参数问题");
            } else if progress > 90.0 {
                log::error!("   - 接近完成时失败，可能是最后几个块的FEC恢复失败");
            } else {
                log::error!("   - 传输中途失败，可能是:");
                log::error!("     * 发送速率过快，接收端缓冲区溢出");
                log::error!("     * 网络丢包率过高，FEC冗余度不足");
                log::error!("     * 接收超时(object_timeout)设置过短");
            }
            if rate_mbps > 100.0 {
                log::error!("   - ⚠️ 速率较高({:.2} Mbps)，建议降低发送速率或增加FEC冗余", rate_mbps);
            }
            log::error!("💡 建议:");
            log::error!("   1. 降低发送端的 send_interval_micros 或 max_rate_kbps");
            log::error!("   2. 增加 FEC 参数 max_number_of_parity_symbols");
            log::error!("   3. 增加接收端的 object_timeout 设置");
            log::error!("   4. 检查网络丢包率（使用Wireshark）");
            log::error!("============================================================");
            
            log::info!("🗑️  删除未完成的文件: {:?}", inner.destination);
            std::fs::remove_file(inner.destination.as_ref().unwrap()).ok();
            inner.destination = None;
        }
    }

    fn interrupted(&self, now: SystemTime) {
        self.error(now);
    }

    fn enable_md5_check(&self) -> bool {
        self.enable_md5_check
    }
}
