pub const DON_MAGIC: [u8; 4] = *b"DON1";
pub const DON_VERSION: u16 = 3;  // v3 default (chunk table)
pub const DON_VERSION_V2: u16 = 2;
pub const DON_VERSION_V3: u16 = 3;  // dedup per chunk
pub const DON_VERSION_V1: u16 = 1;  // backward compat
pub const DON_HEADER_SIZE: u64 = 96;
pub const DON_CHUNK_SIZE: usize = 4096;  // 4KB per chunk
pub const DON_ENTRY_SIZE: u64 = 68;  // v3 default (chunk_table_offset + chunk_count)
pub const DON_ENTRY_SIZE_V2: u64 = 56;  // v2 (dedup_hash)
pub const DON_ENTRY_SIZE_V1: u64 = 48;  // v1 (backward compat)

pub const DON_METHOD_STORED: u16 = 0;
pub const DON_METHOD_RLE: u16 = 1;
pub const DON_METHOD_LZ: u16 = 2;
pub const DON_METHOD_LZ_V5: u16 = 3;
pub const DON_METHOD_LZ_V5_HUFF: u16 = 5;
pub const DON_METHOD_RLE_EXT: u16 = 6;  // Extended RLE (run/literal up to 64KB)

pub const DON_ENTRY_FLAG_FILE: u16 = 0x0001;
pub const DON_ENTRY_FLAG_DEDUP: u16 = 0x0002;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonMethod {
    Stored,
    Rle,
    Lz,
    LzV5,
    LzV5Huff,
    RleExt,
}

impl DonMethod {
    pub fn to_u16(self) -> u16 {
        match self {
            DonMethod::Stored => DON_METHOD_STORED,
            DonMethod::Rle => DON_METHOD_RLE,
            DonMethod::Lz => DON_METHOD_LZ,
            DonMethod::LzV5 => DON_METHOD_LZ_V5,
            DonMethod::LzV5Huff => DON_METHOD_LZ_V5_HUFF,
            DonMethod::RleExt => DON_METHOD_RLE_EXT,
        }
    }

    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            DON_METHOD_STORED => Some(DonMethod::Stored),
            DON_METHOD_RLE => Some(DonMethod::Rle),
            DON_METHOD_LZ => Some(DonMethod::Lz),
            DON_METHOD_LZ_V5 => Some(DonMethod::LzV5),
            DON_METHOD_LZ_V5_HUFF => Some(DonMethod::LzV5Huff),
            DON_METHOD_RLE_EXT => Some(DonMethod::RleExt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DonHeader {
    pub version: u16,
    pub flags: u16,
    pub entry_count: u32,
    pub entry_table_offset: u64,
    pub entry_table_size: u64,
    pub path_table_offset: u64,
    pub path_table_size: u64,
    pub data_offset: u64,
    pub archive_crc32: u32,
    pub chunk_table_offset: u64,  // v3: offset ke chunk table
    pub chunk_table_size: u64,    // v3: ukuran chunk table
}

#[derive(Debug, Clone)]
pub struct DonEntry {
    pub path: String,
    pub flags: u16,
    pub data_offset: u64,
    pub stored_size: u64,
    pub original_size: u64,
    pub crc32: u32,
    pub method: DonMethod,
    pub mtime_unix: u64,
    pub dedup_hash: u64,        // hash file (0 kalau tidak dedup)
    pub dedup_source: Option<usize>,  // index entry pertama (runtime only)
    pub chunk_table_offset: u64,  // v3: offset ke chunk table
    pub chunk_count: u32,         // v3: jumlah chunk
}

#[derive(Debug, Clone)]
pub struct DonRawEntry {
    pub path_offset: u32,
    pub path_len: u16,
    pub flags: u16,
    pub data_offset: u64,
    pub stored_size: u64,
    pub original_size: u64,
    pub crc32: u32,
    pub method: u16,
    pub reserved0: u16,
    pub mtime_unix: u64,
    pub dedup_hash: u64,  // 0 kalau tidak dedup
    pub chunk_table_offset: u64,  // v3
    pub chunk_count: u32,         // v3
}


pub struct DonChunk {
    pub offset: u64,
    pub len: u32,
    pub crc32: u32,
}
