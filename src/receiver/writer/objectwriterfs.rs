use super::{ObjectMetadata, ObjectWriter, ObjectWriterBuilder, ObjectWriterBuilderResult};
use crate::{
    common::udpendpoint::UDPEndpoint,
    error::{FluteError, Result},
};
use std::{cell::RefCell, io::Write, time::SystemTime};

///
/// Write objects received by the `receiver` to a filesystem
///
#[derive(Debug)]
pub struct ObjectWriterFSBuilder {
    dest: std::path::PathBuf, // 基础目标目录
    enable_md5_check: bool,   // 是否启用MD5校验
}

impl ObjectWriterFSBuilder {
    /// Return a new `ObjectWriterBuffer`
    pub fn new(dest: &std::path::Path, enable_md5_check: bool) -> Result<ObjectWriterFSBuilder> {
        if !dest.is_dir() {
            log::error!("Destination path is not a directory: {:?}", dest);
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
        log::debug!(
            "Creating new file writer for object: {:?}",
            meta.content_location
        );
        ObjectWriterBuilderResult::StoreObject(Box::new(ObjectWriterFS {
            dest: self.dest.clone(),
            inner: RefCell::new(ObjectWriterFSInner {
                destination: None,
                writer: None,
                bytes_written: 0, // 新增：记录写入的数据量
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
    dest: std::path::PathBuf, // 基础目标目录
    inner: RefCell<ObjectWriterFSInner>, // 内部状态
    meta: ObjectMetadata,                // 对象元数据
    enable_md5_check: bool,              // 是否启用MD5校验
}

///
///
#[derive(Debug)]
pub struct ObjectWriterFSInner {
    destination: Option<std::path::PathBuf>, // 最终文件路径
    writer: Option<std::io::BufWriter<std::fs::File>>, // 文件写入器
    bytes_written: usize,                    // 新增：记录写入的数据量
}

impl ObjectWriter for ObjectWriterFS {
    fn open(&self, _now: SystemTime) -> Result<()> {
        log::debug!("[OPEN] Starting file open for {:?}", self.meta.content_location);

        let url = url::Url::parse(&self.meta.content_location);
        let content_location_path = match &url {
            Ok(url) => {
                log::trace!("Successfully parsed content location URL: {:?}", url);
                url.path()
            }
            Err(e) => match e {
                url::ParseError::RelativeUrlWithoutBase => {
                    log::debug!(
                        "Using raw content location (relative URL without base): {:?}",
                        self.meta.content_location
                    );
                    &self.meta.content_location
                }
                url::ParseError::RelativeUrlWithCannotBeABaseBase => {
                    log::debug!(
                        "Using raw content location (relative URL cannot be a base): {:?}",
                        self.meta.content_location
                    );
                    &self.meta.content_location
                }
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
        log::trace!("Extracted relative path: {:?}", relative_path);

        let destination = self.dest.join(relative_path);
        log::info!("[OPEN] Creating file at: {:?}", destination);
        // let parent = destination.parent();
        // if parent.is_some() {
        //     let parent = parent.unwrap();
        //     if !parent.is_dir() {
        //         log::debug!("Creating parent directories: {:?}", parent);
        //         std::fs::create_dir_all(parent)?;
        //     }
        // }
        if let Some(parent) = destination.parent() {
            if !parent.is_dir() {
                log::debug!("Creating parent directories: {:?}", parent);
                if let Err(e) = std::fs::create_dir_all(parent) {
                    log::error!("Failed to create parent directories: {:?}", e);
                    return Err(e.into());
                }
                log::debug!("Successfully created parent directories");
            } else {
                log::trace!("Parent directory already exists: {:?}", parent);
            }
        }

        log::debug!("[OPEN] Attempting file creation");
        match std::fs::File::create(&destination) {
            Ok(file) => {
                log::info!("[OPEN] File created successfully");
                let mut inner = self.inner.borrow_mut();
                inner.writer = Some(std::io::BufWriter::with_capacity(
                    16 * 1024 * 1024, // 16MB buffer
                    file
                ));
                inner.destination = Some(destination);
                inner.bytes_written = 0;
                log::debug!("[OPEN] Initialized writer with buffer");
                Ok(())
            }
            Err(e) => {
                log::error!("[OPEN] Failed to create file: {:?}", e);
                // 添加详细的权限检查
                if let Some(parent) = destination.parent() {
                    match std::fs::metadata(parent) {
                        Ok(meta) => log::debug!("[OPEN] Parent dir permissions: {:?}", meta.permissions()),
                        Err(e) => log::warn!("[OPEN] Failed to check parent dir: {:?}", e),
                    }
                }
                Err(e.into())
            }
        }
    }

    fn write(&self, _sbn: u32, data: &[u8], _now: SystemTime) -> Result<()> {
        let mut inner = self.inner.borrow_mut();

        if inner.writer.is_none() {
            log::warn!(
                "Write called but writer is None for {:?}",
                inner.destination
            );
            return Ok(());
        }

        log::trace!(
            "Attempting to write {} bytes to file {:?}",
            data.len(),
            inner.destination
        );

        // 写入数据
        match inner.writer.as_mut().unwrap().write_all(data) {
            Ok(_) => {
                inner.bytes_written += data.len();
                log::trace!(
                    "Successfully wrote {} bytes (total: {})",
                    data.len(),
                    inner.bytes_written
                );

                if inner.bytes_written % (10 * 1024 * 1024) == 0 {
                    log::debug!(
                        "Write progress: {} MB written to {:?}",
                        inner.bytes_written / (1024 * 1024),
                        inner.destination
                    );
                }
                Ok(())
            }
            Err(e) => {
                log::error!(
                    "Failed to write {} bytes to file {:?}: {:?} (total written: {})",
                    data.len(),
                    inner.destination,
                    e,
                    inner.bytes_written
                );
                Err(FluteError::new(format!(
                    "Fail to write data to file {:?} {:?}",
                    inner.destination, e
                )))
            }
        }
    }

    fn complete(&self, _now: SystemTime) {
        let mut inner = self.inner.borrow_mut();

        if inner.writer.is_none() {
            log::warn!(
                "Complete called but writer is None for {:?}",
                inner.destination
            );
            return;
        }

        log::debug!("Flushing buffer for file {:?}", inner.destination);
        // 刷新缓冲区并关闭文件
        match inner.writer.as_mut().unwrap().flush() {
            Ok(_) => {
                log::info!("Successfully flushed file {:?} (total bytes: {})", 
                          inner.destination, inner.bytes_written);
            },
            Err(e) => {
                log::error!("Failed to flush file {:?}: {:?}", inner.destination, e);
            }
        }

        // 获取文件大小
        if let Some(ref path) = inner.destination {
            log::debug!("Checking final file size for {:?}", path);

            match std::fs::metadata(path) {
                Ok(metadata) => {
                    log::info!(
                        "File completion: path={:?}, size_on_disk={} bytes, bytes_written={} bytes",
                        path,
                        metadata.len(),
                        inner.bytes_written
                    );
                    
                    if metadata.len() != inner.bytes_written as u64 {
                        log::warn!(
                            "File size mismatch: disk={} bytes, expected={} bytes (difference: {} bytes)",
                            metadata.len(),
                            inner.bytes_written,
                            metadata.len() as i64 - inner.bytes_written as i64
                        );
                    }
                },
                Err(e) => {
                    log::error!("Failed to get metadata for {:?}: {:?}", path, e);
                }
            }
        }

        log::debug!("Releasing writer resources for {:?}", inner.destination);
        inner.writer = None;
        inner.destination = None;
    }

    // fn error(&self, _now: SystemTime) {
    //     let mut inner = self.inner.borrow_mut(); // 获取内部状态的可变引用
    //     inner.writer = None; // 释放文件写入器资源

    //     // if inner.destination.is_some() {
    //     //     // 检查是否存在目标文件路径
    //     //     log::error!("Remove file {:?}", inner.destination);
    //     //     std::fs::remove_file(inner.destination.as_ref().unwrap()).ok(); // 删除文件
    //     //     inner.destination = None; // 清空目标路径
    //     // }
    // }

    // fn error(&self, _now: SystemTime) {
    //     let mut inner = self.inner.borrow_mut();
    //     log::error!(
    //         "Error occurred while processing file {:?} (bytes_written: {})",
    //         inner.destination,
    //         inner.bytes_written
    //     );

    //     if let Some(ref path) = inner.destination {
    //         log::debug!("Checking current file state for {:?}", path);
    //         match std::fs::metadata(path) {
    //             Ok(metadata) => {
    //                 log::error!(
    //                     "File state: path={:?}, size_on_disk={} bytes, bytes_written={} bytes",
    //                     path,
    //                     metadata.len(),
    //                     inner.bytes_written
    //                 );
                    
    //                 if metadata.len() == 0 {
    //                     log::warn!("File is empty on disk but bytes_written={}", inner.bytes_written);
    //                 }
    //             },
    //             Err(e) => {
    //                 log::error!("Failed to get file metadata for {:?}: {:?}", path, e);
    //             }
    //         }
    //     }

    //     if let Some(writer) = inner.writer.as_mut() {
    //         log::debug!("Attempting to flush writer before releasing");
    //         if let Err(e) = writer.flush() {
    //             log::error!("Failed to flush writer during error handling: {:?}", e);
    //         }
    //     }

    //     log::debug!("Releasing writer resources due to error");
    //     inner.writer = None;
    // }

    fn error(&self, _now: SystemTime) {
        let mut inner = self.inner.borrow_mut();
        inner.writer = None;
        if inner.destination.is_some() {
            log::error!("Remove file {:?}", inner.destination);
            std::fs::remove_file(inner.destination.as_ref().unwrap()).ok();
            inner.destination = None;
        }
    }
    
    fn interrupted(&self, now: SystemTime) {
        log::warn!("Object reception interrupted");
        self.error(now);
    }

    fn enable_md5_check(&self) -> bool {
        self.enable_md5_check
    }
}
