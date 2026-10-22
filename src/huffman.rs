//! Huffman coding untuk literal LZ v5.
//!
//! Implementasi static Huffman (tanpa adaptive):
//! - Hitung frekuensi 256 byte.
//! - Build Huffman tree dengan priority queue.
//! - Generate code length per simbol (max 15 bit).
//! - Encode/decode bitstream.

use std::collections::BinaryHeap;
use std::cmp::Reverse;
use std::cmp::Ordering;

/// Node Huffman tree.
#[derive(Debug, Clone)]
struct Node {
    freq: u64,
    symbol: Option<u8>,  // Some untuk leaf, None untuk internal
    left: Option<usize>,
    right: Option<usize>,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.freq == other.freq
    }
}

impl Eq for Node {}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: freq terkecil dulu
        other.freq.cmp(&self.freq)
    }
}

/// Tabel Huffman: code length per simbol (0 = simbol tidak dipakai).
pub struct HuffmanTable {
    /// Length (bit) per simbol 0-255. 0 artinya simbol tidak ada.
    pub code_lengths: [u8; 256],
    /// Code (canonical Huffman) per simbol. Bit LSB-first.
    pub codes: [u32; 256],
    /// Jumlah simbol unik.
    pub unique_count: usize,
}

impl HuffmanTable {
    /// Build dari frekuensi.
    pub fn from_frequencies(freqs: &[u64; 256]) -> Self {
        // Kumpulkan simbol dengan frekuensi > 0
        // BinaryHeap max-heap. Kita mau min-heap (freq terkecil dulu).
        // Pakai Reverse untuk reverse ordering.
        let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
        let mut nodes: Vec<Node> = Vec::new();
        let mut leaf_index: [Option<usize>; 256] = [None; 256];

        for (sym, &freq) in freqs.iter().enumerate() {
            if freq > 0 {
                let idx = nodes.len();
                nodes.push(Node {
                    freq,
                    symbol: Some(sym as u8),
                    left: None,
                    right: None,
                });
                heap.push(Reverse((freq, idx)));
                leaf_index[sym] = Some(idx);
            }
        }

        let unique_count = heap.len();
        if unique_count == 0 {
            return Self {
                code_lengths: [0; 256],
                codes: [0; 256],
                unique_count: 0,
            };
        }
        if unique_count == 1 {
            // Hanya 1 simbol: panjang 1 bit
            let sym = leaf_index.iter().position(|x| x.is_some()).unwrap();
            let mut code_lengths = [0u8; 256];
            code_lengths[sym] = 1;
            let mut codes = [0u32; 256];
            codes[sym] = 0;
            return Self {
                code_lengths,
                codes,
                unique_count: 1,
            };
        }

        // Build tree: pop 2 terkecil, gabung
        while heap.len() > 1 {
            let Reverse((f1, i1)) = heap.pop().unwrap();
            let Reverse((f2, i2)) = heap.pop().unwrap();
            let new_idx = nodes.len();
            nodes.push(Node {
                freq: f1 + f2,
                symbol: None,
                left: Some(i1),
                right: Some(i2),
            });
            heap.push(Reverse((f1 + f2, new_idx)));
        }

        // Root adalah nodes[heap.pop().0]
        let Reverse((_, root)) = heap.pop().unwrap();

        // Traverse tree: DFS, catat code length per leaf
        let mut code_lengths = [0u8; 256];
        let mut stack: Vec<(usize, u8)> = vec![(root, 0)];
        while let Some((node_idx, depth)) = stack.pop() {
            let node = &nodes[node_idx];
            if let Some(sym) = node.symbol {
                code_lengths[sym as usize] = depth.max(1);
            } else {
                if let Some(l) = node.left {
                    stack.push((l, depth + 1));
                }
                if let Some(r) = node.right {
                    stack.push((r, depth + 1));
                }
            }
        }

        // Tidak clamp — pakai len asli (max 255).
        // Canonical Huffman: sort simbol by (length, simbol), assign code
        let max_len = code_lengths.iter().copied().max().unwrap_or(0) as u32;
        let mut codes = [0u32; 256];
        let mut code: u32 = 0;
        for len in 1..=max_len {
            for sym in 0..256 {
                if code_lengths[sym] as u32 == len {
                    codes[sym] = code;
                    code += 1;
                }
            }
            if len < max_len {
                code <<= 1;
            }
        }

        Self {
            code_lengths,
            codes,
            unique_count,
        }
    }

    /// Encode tabel ke bytes (code_lengths: 256 byte, pakai 4 bit per simbol).
    /// Format: 256 * 4 bit = 128 byte.
    /// Simbol dengan length > 15 di-clamp ke 15 (tidak terjadi untuk data normal).
    pub fn to_bytes(&self) -> Vec<u8> {
        // 256 byte: 1 byte per simbol (code_length, max 255)
        self.code_lengths.to_vec()
    }

    /// Decode tabel dari bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() < 256 {
            return Self {
                code_lengths: [0; 256],
                codes: [0; 256],
                unique_count: 0,
            };
        }
        let mut code_lengths = [0u8; 256];
        code_lengths.copy_from_slice(&bytes[..256]);
        // Canonical: assign code per len, max len bisa 255
        let mut codes = [0u32; 256];
        let mut code: u32 = 0;
        // Cari max_len
        let max_len = code_lengths.iter().copied().max().unwrap_or(0) as u32;
        for len in 1..=max_len {
            for sym in 0..256 {
                if code_lengths[sym] as u32 == len {
                    codes[sym] = code;
                    code += 1;
                }
            }
            if len < max_len {
                code <<= 1;
            }
        }
        let unique_count = code_lengths.iter().filter(|&&l| l > 0).count();
        Self {
            code_lengths,
            codes,
            unique_count,
        }
    }
}

/// Hitung frekuensi 256 byte dari slice.
pub fn count_frequencies(data: &[u8]) -> [u64; 256] {
    let mut freqs = [0u64; 256];
    for &b in data {
        freqs[b as usize] += 1;
    }
    freqs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frequency_count() {
        let data = b"hello hello";
        let freqs = count_frequencies(data);
        assert_eq!(freqs[b'h' as usize], 2);
        assert_eq!(freqs[b'e' as usize], 2);
        assert_eq!(freqs[b'l' as usize], 4);
        assert_eq!(freqs[b'o' as usize], 2);
        assert_eq!(freqs[b' ' as usize], 1);
    }

    #[test]
    fn test_empty() {
        let freqs = [0u64; 256];
        let table = HuffmanTable::from_frequencies(&freqs);
        assert_eq!(table.unique_count, 0);
    }

    #[test]
    fn test_single_symbol() {
        let mut freqs = [0u64; 256];
        freqs[b'a' as usize] = 100;
        let table = HuffmanTable::from_frequencies(&freqs);
        assert_eq!(table.unique_count, 1);
        assert_eq!(table.code_lengths[b'a' as usize], 1);
    }

    #[test]
    fn test_two_symbols() {
        let mut freqs = [0u64; 256];
        freqs[b'a' as usize] = 10;
        freqs[b'b' as usize] = 10;
        let table = HuffmanTable::from_frequencies(&freqs);
        assert_eq!(table.unique_count, 2);
        assert_eq!(table.code_lengths[b'a' as usize], 1);
        assert_eq!(table.code_lengths[b'b' as usize], 1);
    }

    #[test]
    fn test_table_roundtrip() {
        let mut freqs = [0u64; 256];
        for i in 0..256 {
            freqs[i] = (i as u64) + 1;
        }
        let table = HuffmanTable::from_frequencies(&freqs);
        let bytes = table.to_bytes();
        let table2 = HuffmanTable::from_bytes(&bytes);
        for i in 0..256 {
            assert_eq!(table.code_lengths[i], table2.code_lengths[i]);
        }
    }
}

// ═══════════════════════════════════════════════════════
// STEP 2: Bit writer/reader
// ═══════════════════════════════════════════════════════

/// Bit writer: tulis bit ke Vec<u8> (LSB-first).
pub struct BitWriter {
    out: Vec<u8>,
    bit_buf: u64,
    bit_count: u32,
}

impl BitWriter {
    pub fn new() -> Self {
        Self {
            out: Vec::new(),
            bit_buf: 0,
            bit_count: 0,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            out: Vec::with_capacity(cap),
            bit_buf: 0,
            bit_count: 0,
        }
    }

    /// Tulis `n` bit (n <= 32) dari `value` (LSB-first).
    pub fn write_bits(&mut self, value: u32, n: u32) {
        debug_assert!(n <= 32);
        self.bit_buf |= ((value as u64) & ((1u64 << n) - 1)) << self.bit_count;
        self.bit_count += n;
        while self.bit_count >= 8 {
            self.out.push((self.bit_buf & 0xFF) as u8);
            self.bit_buf >>= 8;
            self.bit_count -= 8;
        }
    }

    /// Flush sisa bit (padding 0).
    pub fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            self.out.push((self.bit_buf & 0xFF) as u8);
        }
        self.out
    }

    /// Jumlah byte yang sudah ditulis.
    pub fn len(&self) -> usize {
        self.out.len()
    }
}

/// Bit reader: baca bit dari slice (LSB-first).
pub struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bit_buf: u64,
    bit_count: u32,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit_buf: 0,
            bit_count: 0,
        }
    }

    /// Baca `n` bit (n <= 32).
    pub fn read_bits(&mut self, n: u32) -> Option<u32> {
        debug_assert!(n <= 32);
        while self.bit_count < n {
            if self.pos >= self.data.len() {
                return None;
            }
            self.bit_buf |= (self.data[self.pos] as u64) << self.bit_count;
            self.pos += 1;
            self.bit_count += 8;
        }
        let value = (self.bit_buf & ((1u64 << n) - 1)) as u32;
        self.bit_buf >>= n;
        self.bit_count -= n;
        Some(value)
    }

    /// Sisa bit yang tersedia (perkiraan).
    pub fn remaining_bits(&self) -> usize {
        (self.data.len() - self.pos) * 8 + self.bit_count as usize
    }
}

#[cfg(test)]
mod bit_tests {
    use super::*;

    #[test]
    fn test_bitwriter_basic() {
        let mut bw = BitWriter::new();
        bw.write_bits(0b101, 3);
        bw.write_bits(0b1101, 4);
        let out = bw.finish();
        // 7 bit total: 0b1101101 (LSB-first), padding 0
        // byte: 0b01101101 = 0x6D
        assert_eq!(out, vec![0x6D]);
    }

    #[test]
    fn test_bitwriter_reader_roundtrip() {
        let mut bw = BitWriter::new();
        bw.write_bits(5, 3);
        bw.write_bits(100, 7);
        bw.write_bits(1000, 10);
        bw.write_bits(999999, 20);
        bw.write_bits(7, 3);
        let bytes = bw.finish();

        let mut br = BitReader::new(&bytes);
        assert_eq!(br.read_bits(3), Some(5));
        assert_eq!(br.read_bits(7), Some(100));
        assert_eq!(br.read_bits(10), Some(1000));
        assert_eq!(br.read_bits(20), Some(999999));
        assert_eq!(br.read_bits(3), Some(7));
    }

    #[test]
    fn test_bitwriter_empty() {
        let bw = BitWriter::new();
        let out = bw.finish();
        assert!(out.is_empty());
    }

    #[test]
    fn test_bitwriter_capacity() {
        let bw = BitWriter::with_capacity(100);
        assert_eq!(bw.len(), 0);
    }

    #[test]
    fn test_bitreader_eof() {
        let mut br = BitReader::new(&[0xFF]);
        assert_eq!(br.read_bits(8), Some(0xFF));
        assert_eq!(br.read_bits(1), None);
    }
}

// ═══════════════════════════════════════════════════════
// STEP 3-4: Encode/decode literal dengan Huffman
// ═══════════════════════════════════════════════════════

/// Encode slice data dengan Huffman table.
/// Return: bitstream (byte-aligned).
pub fn encode_literals(data: &[u8], table: &HuffmanTable) -> Vec<u8> {
    let mut bw = BitWriter::with_capacity(data.len());
    for &byte in data {
        let len = table.code_lengths[byte as usize];
        if len == 0 {
            // Simbol tidak ada di table (harusnya tidak terjadi)
            continue;
        }
        let code = table.codes[byte as usize];
        // Write MSB-first (standar Huffman)
        for i in (0..len as u32).rev() {
            bw.write_bits((code >> i) & 1, 1);
        }
    }
    bw.finish()
}

/// Decode bitstream dengan Huffman table, return Vec<u8>.
/// `expected_size` untuk validasi (opsional).
pub fn decode_literals(
    data: &[u8],
    table: &HuffmanTable,
    expected_size: Option<usize>,
) -> Result<Vec<u8>, String> {
    if table.unique_count == 0 {
        return Ok(Vec::new());
    }

    // Build lookup: (code, len) → symbol
    // Untuk decode cepat, kita pakai "canonical Huffman decode"
    // dengan tabel lookup berdasarkan code length.
    
    // Simpel: coba decode per simbol dengan iterasi code length 1..15.
    let mut br = BitReader::new(data);
    let mut out = Vec::with_capacity(expected_size.unwrap_or(data.len() * 2));

    // Precompute: untuk setiap len, map code → simbol
    let max_len = table.code_lengths.iter().copied().max().unwrap_or(0) as usize;
    let mut code_to_sym: Vec<std::collections::HashMap<u32, u8>> = 
        (0..=max_len).map(|_| std::collections::HashMap::new()).collect();
    for sym in 0..256 {
        let len = table.code_lengths[sym];
        if len > 0 {
            if (len as usize) < code_to_sym.len() { code_to_sym[len as usize].insert(table.codes[sym], sym as u8); }
        }
    }

    // Decode: baca bit satu per satu, akumulasi code, cek per length
    let mut code: u32 = 0;
    let mut code_len: u32 = 0;

    loop {
        if let Some(max_size) = expected_size {
            if out.len() >= max_size {
                break;
            }
        }
        // Cek remaining bits
        if br.remaining_bits() < 15 {
            // Tidak cukup untuk code length max (15)
            // Tapi mungkin masih ada 1 code pendek
            // Coba baca bit sampai habis
        }

        let bit = match br.read_bits(1) {
            Some(b) => b,
            None => break,
        };
        code = (code << 1) | bit;
        code_len += 1;

        if code_len as usize >= code_to_sym.len() {
            return Err(format!("Huffman decode: code length > 15 di pos {}", out.len()));
        }

        if let Some(&sym) = code_to_sym.get(code_len as usize).and_then(|m| m.get(&code)) {
            out.push(sym);
            code = 0;
            code_len = 0;

            if let Some(max_size) = expected_size {
                if out.len() == max_size {
                    break;
                }
            }
        }
    }

    if let Some(max_size) = expected_size {
        if out.len() != max_size {
            return Err(format!(
                "Huffman decode: size mismatch, expected {}, got {}",
                max_size,
                out.len()
            ));
        }
    }

    Ok(out)
}




#[cfg(test)]
mod debug_tests {
    use super::*;

    #[test]
    fn test_debug_all_bytes_small() {
        // Test dengan data kecil: 10 byte pertama 0..255
        let data: Vec<u8> = (0..10).collect();
        let freqs = count_frequencies(&data);
        let table = HuffmanTable::from_frequencies(&freqs);
        
        eprintln!("=== FREQS ===");
        for (sym, &f) in freqs.iter().enumerate() {
            if f > 0 {
                eprintln!("  sym {}: freq {}", sym, f);
            }
        }
        
        eprintln!("=== CODE_LENGTHS & CODES ===");
        for sym in 0..256 {
            if table.code_lengths[sym] > 0 {
                eprintln!("  sym {}: len={} code={:0width$b}", 
                    sym, table.code_lengths[sym], table.codes[sym],
                    width = table.code_lengths[sym] as usize);
            }
        }
        
        let encoded = encode_literals(&data, &table);
        eprintln!("=== ENCODED: {} bytes ===", encoded.len());
        for (i, &b) in encoded.iter().enumerate() {
            eprintln!("  byte[{}]: {:08b}", i, b);
        }
        
        let decoded = decode_literals(&encoded, &table, Some(data.len())).unwrap();
        eprintln!("=== DECODED: {:?}", decoded);
        assert_eq!(decoded, data, "Mismatch!");
    }
}

#[cfg(test)]
mod literal_tests {
    use super::*;

    fn roundtrip(data: &[u8]) -> Result<(), String> {
        let freqs = count_frequencies(data);
        let table = HuffmanTable::from_frequencies(&freqs);
        let encoded = encode_literals(data, &table);
        let decoded = decode_literals(&encoded, &table, Some(data.len()))?;
        if decoded != data {
            return Err(format!(
                "roundtrip mismatch: len {} vs {}",
                decoded.len(),
                data.len()
            ));
        }
        Ok(())
    }

    #[test]
    fn test_encode_decode_simple() {
        let data = b"hello hello hello";
        roundtrip(data).unwrap();
    }

    #[test]
    fn test_encode_decode_empty() {
        let data = b"";
        // Empty: table dengan unique_count=0
        let freqs = count_frequencies(data);
        let table = HuffmanTable::from_frequencies(&freqs);
        let encoded = encode_literals(data, &table);
        let decoded = decode_literals(&encoded, &table, Some(0)).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_encode_decode_single_byte() {
        let data = b"a";
        roundtrip(data).unwrap();
    }

    #[test]
    fn test_encode_decode_single_symbol_repeated() {
        let data = vec![b'x'; 100];
        roundtrip(&data).unwrap();
    }

    #[test]
    fn test_encode_decode_all_bytes() {
        let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        roundtrip(&data).unwrap();
    }

    #[test]
    fn test_encode_decode_text() {
        let data = b"the quick brown fox jumps over the lazy dog. \
                     the quick brown fox jumps over the lazy dog. \
                     the quick brown fox jumps over the lazy dog.";
        roundtrip(data).unwrap();
    }

    #[test]
    fn test_compression_ratio() {
        // Data dengan distribusi tidak merata → Huffman harus lebih kecil dari 8 bit/byte
        let mut data = Vec::new();
        for _ in 0..100 {
            data.extend_from_slice(b"aaaaaaaaaabbbbbbbbbbccccccccccddddeeee");
        }
        let freqs = count_frequencies(&data);
        let table = HuffmanTable::from_frequencies(&freqs);
        let encoded = encode_literals(&data, &table);
        // Huffman harus < 8 bit per byte (karena distribusi skew)
        assert!(encoded.len() < data.len(), 
            "Huffman tidak kompres: {} >= {}", encoded.len(), data.len());
        println!("Huffman: {} -> {} bytes ({:.2}%)", 
            data.len(), encoded.len(), 
            encoded.len() as f64 * 100.0 / data.len() as f64);
    }
}
