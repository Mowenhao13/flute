// param_test_rs.rs
use num_integer::{div_ceil, div_floor};
// KB, MB 定义
const KB: usize = 1024;
const MB: usize = 1024 * 1024;
const MAX_TRANSFER_LENGTH: usize = 0xFFFFFFFFFFFF; // 48 bits max

fn main() {
    // Transfer_length: 文件传输总长度
    let transfer_length: u64 = 1024 * MB;
    // 输入参数
    // Encoding_symbol_length: 每个符号的字节数
    let encoding_symbol_length: u16 = None;
    // Maximum_source_block_length: 单个源块的最大符号数量
    let maximum_source_block_length: u32 = None;
    // Max_number_of_parity_symbols(nb_parity_symbols): 冗余符号数量
    let max_number_of_parity_symbols: u8 = None;

    // 针对ReedSolomonGF28的最大源块数量
    // ReedSolomonGF28UnderSpecified则是u32::MAX
    let max_source_block_number = u8::MAX as usize;

    let block_size = encoding_symbol_length as usize * maximum_source_block_length as usize;

    let size = block_size * max_source_block_number as usize;

    // 确保transfer_length <= max_transfer_length
    let mut max_transfer_length = size;
    if size > MAX_TRANSFER_LENGTH as usize {
        max_transfer_length = MAX_TRANSFER_LENGTH as usize;
    }


    // Nb_source_symbols: 当前数据块包含的源符号的数量
    let nb_source_symbols: usize = num_integer::div_ceil(buffer_len, encoding_symbol_length) as usize;
    // Nb_parity_symbols: 当前数据块包含的荣誉符号数量
    let nb_parity_symbols: usize = None;
    // Transfer_length_after: 编码后的传输文件尺寸
    // 必须保证transfer_length_after < MAX_TRANSFER_LENGTH
    let transfer_length_after: usize = transfer_length + (nb_source_symbols * encoding_symbol_length as usize);
}