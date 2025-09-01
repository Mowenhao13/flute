use crate::common::{
    alc,
    oti::{self, SchemeSpecific},
};
use crate::error::FluteError;
use crate::fec;
use crate::fec::nocode;
use crate::fec::rscodec;
use crate::fec::FecDecoder;
use crate::tools::error::Result;

#[derive(Debug)]
pub struct BlockDecoder {
    pub completed: bool,       // 是否已完成解码
    pub initialized: bool,     // 是否已初始化
    pub block_size: usize,     // 块大小(字节)
    decoder: Option<Box<dyn FecDecoder>>, // 动态派发的FEC解码器
}

impl BlockDecoder {
    pub fn new() -> BlockDecoder {
        BlockDecoder {
            completed: false,
            initialized: false,
            decoder: None,
            block_size: 0,
        }
    }

    pub fn init(
        &mut self,
        oti: &oti::Oti,           // 传输对象信息
        nb_source_symbols: u32,   // 源符号数量
        block_size: usize,        // 块大小
        sbn: u32                  // 源块编号
    ) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        // 根据不同的 FEC 编码类型初始化相应的解码器
        match oti.fec_encoding_id {
            oti::FECEncodingID::NoCode => {
                let codec = nocode::NoCodeDecoder::new(nb_source_symbols as usize);
                self.decoder = Some(Box::new(codec));
            }
            oti::FECEncodingID::ReedSolomonGF28 => {
                let codec = rscodec::RSGalois8Codec::new(
                    nb_source_symbols as usize,
                    oti.max_number_of_parity_symbols as usize,
                    oti.encoding_symbol_length as usize,
                )?;
                self.decoder = Some(Box::new(codec));
            }
            oti::FECEncodingID::ReedSolomonGF28UnderSpecified => {
                let codec = rscodec::RSGalois8Codec::new(
                    nb_source_symbols as usize,
                    oti.max_number_of_parity_symbols as usize,
                    oti.encoding_symbol_length as usize,
                )?;
                self.decoder = Some(Box::new(codec));
            }
            oti::FECEncodingID::ReedSolomonGF2M => {
                log::warn!("Not implemented")
            }
            oti::FECEncodingID::RaptorQ => {
                if let Some(SchemeSpecific::RaptorQ(scheme)) = oti.scheme_specific.as_ref() {
                    let codec = fec::raptorq::RaptorQDecoder::new(
                        sbn,
                        nb_source_symbols as usize,
                        oti.encoding_symbol_length as usize,
                        scheme,
                    );
                    self.decoder = Some(Box::new(codec));
                } else {
                    return Err(FluteError::new("RaptorQ Scheme not found"));
                }
            }
            oti::FECEncodingID::Raptor => {
                if oti.scheme_specific.is_none() {
                    return Err(FluteError::new("Raptor Scheme not found"));
                }

                let codec = fec::raptor::RaptorDecoder::new(nb_source_symbols as usize, block_size);
                self.decoder = Some(Box::new(codec));
            }
        }

        self.initialized = true;
        self.block_size = block_size;
        Ok(())
    }

    pub fn source_block(&self) -> Result<&[u8]> {
        if self.decoder.is_none() {
            return Err(FluteError::new("Fail to decode block"));
        }

        self.decoder.as_ref().unwrap().source_block()
    }

    // 显式释放解码器资源，避免内存泄漏
    pub fn deallocate(&mut self) {
        self.decoder = None;
        self.block_size = 0;
    }

    // 接收符号
    // 处理流程：
    // 1.从数据包中提取有效载荷
    // 2.根据ESI(编码符号标识符)将符号送入解码器
    // 3.检查是否已收集足够符号进行解码
    pub fn push(&mut self, pkt: &alc::AlcPkt, payload_id: &alc::PayloadID) {
        debug_assert!(self.initialized);

        if self.completed {
            return;
        }

        let payload = &pkt.data[pkt.data_payload_offset..];
        let decoder = self.decoder.as_mut().unwrap();
        decoder.push_symbol(payload, payload_id.esi);

        if decoder.can_decode() {
            self.completed = decoder.decode();
            if self.completed {
                log::debug!("Block completed");
            }
        }
    }
}
