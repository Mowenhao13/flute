// param_test.rs
use num_integer::{div_ceil, div_floor};
// // KB, MB 定义
const KB = 1024;
const MB = 1024 * 1024;
const MAX_TRANSFER_LENGTH: usize = 0xFFFFFFFFFFF; // 40 bits max;
fn main() {
    // Transfer_length: 文件传输总长度
    let transfer_length: u64 = 1024 * MB;
    // Encoding_symbol_length: 每个符号的字节数
    let encoding_symbol_length: u64 = None;
    // Maximum_source_block_length: 单个源块的最大符号数量
    let maximum_source_block_length: u64 = None;
    // Symbol_count: 总符号数(编码前)
    let symbol_count: u64 = num_integer::div_ceil(transfer_length, encoding_symbol_length);
    // Nb_block: 总块数
    let nb_block: u64 = num_integer::div_ceil(symbol_count, maximum_source_block_length);
    // A_large: 大块尺寸
    let a_large: u64 = div_ceil(nb_block, symbol_count);
    // A_small: 小块尺寸
    let a_small: u64 = div_floor(nb_block, symbol_count);
    // Nb_a_large: 大块数量
    let nb_a_large: u64 = symbol_count - (a_small * nb_block);
    // Block_length: 当前块的符号数量（决定使用大块还是小块尺寸）
    /// ```rust
    /// let block_length = match self.curr_sbn as u64 {
    ///      value if value < self.nb_a_large => self.a_large,
    ///      _ => self.a_small,
    ///  };
    /// ```
    // 小块
    let block_length_small: u64 = a_small;
    // 大块
    let block_length_large: u64 = a_large;
    // Buffer_len: 发送缓冲区长度(假设大部分情况下offset_end <= content.len())
    // 就是假设文件分块均匀, 以主块为主
    let buffer_len: u64 = block_length_large - encoding_symbol_length;
    // Nb_source_symbols: 当前数据块包含的源符号的数量
    let nb_source_symbols: usize = num_integer::div_ceil(buffer_len, encoding_symbol_length) as usize;
    // Max_number_of_parity_symbols(nb_parity_symbols): 冗余符号数量
    let nb_parity_symbols: usize = None;
    // Transfer_length_after: 编码后的传输文件尺寸
    // 必须保证transfer_length_after < MAX_TRANSFER_LENGTH
    let transfer_length_after: usize = transfer_length + (nb_source_symbols * encoding_symbol_length as usize);
    // Sub_blocks_length: 子块长度
    let sub_blocks_length: u16 = None;


}

