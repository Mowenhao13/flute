use super::{ObjectMetadata, ObjectWriter, ObjectWriterBuilder, ObjectWriterBuilderResult};
use crate::{common::udpendpoint::UDPEndpoint, tools::error::Result};
use std::{cell::RefCell, rc::Rc, time::SystemTime};

///
/// Write objects received by the `receiver` to a buffers
///
#[derive(Debug)]
pub struct ObjectWriterBufferBuilder {
    /// List of all objects received
    pub objects: RefCell<Vec<Rc<RefCell<ObjectWriterBuffer>>>>, // 所有接收对象的集合
    /// True when MD5 check is enabled 
    pub enable_md5_check: bool, // 是否启用MD5校验
}

///
/// Write a FLUTE object to a buffer
///
#[derive(Debug)]
struct ObjectWriterBufferWrapper {
    inner: Rc<RefCell<ObjectWriterBuffer>>, 
    enable_md5_check: bool,
}

#[derive(Debug)]
/// Object stored in a buffer
pub struct ObjectWriterBuffer {
    /// true when the object is fully received
    pub complete: bool, // 对象是否接收完成
    /// true when an error occured during the reception
    pub error: bool, // 是否发生错误
    /// buffer containing the data of the object
    pub data: Vec<u8>, // 存储实际数据的缓冲区
    /// Metadata of the object
    pub meta: ObjectMetadata, // 对象元数据
    /// Time when the object reception started
    pub start_time: SystemTime, // 开始接收时间
    /// Time when the object reception ended
    pub end_time: Option<SystemTime>, // 结束接收时间
}

impl ObjectWriterBufferBuilder {
    /// Return a new `ObjectWriterBuffer`
    pub fn new(enable_md5_check: bool) -> ObjectWriterBufferBuilder {
        ObjectWriterBufferBuilder {
            objects: RefCell::new(Vec::new()),
            enable_md5_check,
        }
    }
}

impl Default for ObjectWriterBufferBuilder {
    fn default() -> Self {
        Self::new(true)
    }
}

impl ObjectWriterBuilder for ObjectWriterBufferBuilder {
    fn new_object_writer(
        &self,
        _endpoint: &UDPEndpoint, // 忽略网络端点信息
        _tsi: &u64, // 忽略传输会话ID
        _toi: &u128, // 忽略传输对象ID  
        meta: &ObjectMetadata, // 对象元数据
        now: std::time::SystemTime, // 当前时间
    ) -> ObjectWriterBuilderResult {
        let obj = Rc::new(RefCell::new(ObjectWriterBuffer {
            complete: false,      // 初始状态：未完成
            error: false,         // 初始状态：无错误
            data: Vec::new(),     // 空数据缓冲区
            meta: meta.clone(),   // 克隆元数据
            start_time: now,      // 记录开始时间
            end_time: None        // 结束时间未设置
        }));

        // 创建包装器
        let obj_wrapper = Box::new(ObjectWriterBufferWrapper {
            inner: obj.clone(),
            enable_md5_check: self.enable_md5_check,
        });

        // 将对象加入管理列表
        self.objects.borrow_mut().push(obj);

        // 返回包装器
        ObjectWriterBuilderResult::StoreObject(obj_wrapper)
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

// 实际执行内存写入操作的实现
impl ObjectWriter for ObjectWriterBufferWrapper {
    // 空操作，内存写入不需要预处理
    fn open(&self, _now: SystemTime) -> Result<()> {
        Ok(())
    }

    // 写入数据到内存缓冲区
    fn write(&self, _sbn: u32, data: &[u8], _now: SystemTime) -> Result<()> {
        let mut inner = self.inner.borrow_mut();
        inner.data.extend(data);
        Ok(())
    }

    // 标记对象接收完成
    fn complete(&self, now: SystemTime) {
        let mut inner = self.inner.borrow_mut();
        log::info!("Object complete !");
        inner.complete = true;
        inner.end_time = Some(now);
    }

    // 标记错误状态
    fn error(&self, now: SystemTime) {
        let mut inner = self.inner.borrow_mut();
        log::error!("Object received with error");
        inner.error = true;
        inner.end_time = Some(now);
    }

    // 中断处理（与错误处理相同）
    fn interrupted(&self, now: SystemTime) {
        let mut inner = self.inner.borrow_mut();
        log::error!("Object reception interrupted");
        inner.error = true;
        inner.end_time = Some(now);
    }

    // 返回MD5检查配置
    fn enable_md5_check(&self) -> bool {
        self.enable_md5_check
    }
}
