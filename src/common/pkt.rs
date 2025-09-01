use super::lct;

#[derive(Debug)]
pub struct Pkt {
    pub payload: Vec<u8>, // 实际传输的数据内容（编码后的符号数据） 
    pub transfer_length: u64, // 传输对象的总长度（字节）
    pub esi: u32, // Encoding Symbol Identifier (编码符号标识符) - 标识该符号在块中的位置
    pub sbn: u32, // Source Block Number (源块编号) - 标识该符号属于哪个源块
    pub toi: u128, // Transport Object Identifier (传输对象标识符) - 标识该包属于哪个传输对象
    pub fdt_id: Option<u32>, // 文件描述表实例ID（如果是FDT包）
    pub cenc: lct::Cenc, // 内容编码方式（如gzip/zlib等）
    pub inband_cenc: bool, // 内容编码信息是否在带内传输
    pub close_object: bool, // 是否关闭对象传输的标志
    pub source_block_length: u32, // 源块长度（符号数）
    pub sender_current_time: bool, // 是否包含发送方当前时间
}
