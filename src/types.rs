pub const DON_MAGIC: [u8; 4] = *b"DON1";
pub const DON_VERSION: u16 = 1;
pub const DON_HEADER_SIZE: u64 = 96;
pub const DON_ENTRY_SIZE: u64 = 48;

pub const DON_METHOD_STORED: u16 = 0;
pub const DON_METHOD_RLE: u16 = 1;
pub const DON_METHOD_LZ: u16 = 2;
pub const DON_METHOD_LZ_V5: u16 = 3;

pub const DON_ENTRY_FLAG_FILE: u16 = 0x0001;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DonMethod {
    Stored,
    Rle,
    Lz,
    LzV5,
}

impl DonMethod {
    pub fn to_u16(self) -> u16 {
        match self {
            DonMethod::Stored => DON_METHOD_STORED,
            DonMethod::Rle => DON_METHOD_RLE,
            DonMethod::Lz => DON_METHOD_LZ,
            DonMethod::LzV5 => DON_METHOD_LZ_V5,
        }
    }

    pub fn from_u16(value: u16) -> Option<Self> {
        match value {
            DON_METHOD_STORED => Some(DonMethod::Stored),
            DON_METHOD_RLE => Some(DonMethod::Rle),
            DON_METHOD_LZ => Some(DonMethod::Lz),
            DON_METHOD_LZ_V5 => Some(DonMethod::LzV5),
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
}
