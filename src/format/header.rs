use std::io::{Read, Write};

use crate::error::{DolinError, Result};
use crate::types::{DonHeader, DON_HEADER_SIZE, DON_MAGIC, DON_VERSION, DON_VERSION_V1};

const RESERVED_LEN: usize = 32;

impl DonHeader {
    pub fn new(
        entry_count: u32,
        entry_table_offset: u64,
        entry_table_size: u64,
        path_table_offset: u64,
        path_table_size: u64,
        data_offset: u64,
        archive_crc32: u32,
    ) -> Self {
        Self {
            version: DON_VERSION,
            flags: 0,
            entry_count,
            entry_table_offset,
            entry_table_size,
            path_table_offset,
            path_table_size,
            data_offset,
            archive_crc32,
            chunk_table_offset: 0,
            chunk_table_size: 0,
        }
    }

    pub fn with_flags(mut self, flags: u16) -> Self {
        self.flags = flags;
        self
    }

    pub fn validate_basic(&self, file_len: u64) -> Result<()> {
        if self.version != DON_VERSION && self.version != DON_VERSION_V1 {
            return Err(DolinError::InvalidFormat("unsupported DON version"));
        }

        if self.entry_table_offset < DON_HEADER_SIZE {
            return Err(DolinError::InvalidFormat(
                "entry table offset is before header end",
            ));
        }

        checked_range(self.entry_table_offset, self.entry_table_size, file_len)
            .map_err(|_| DolinError::InvalidFormat("entry table range is out of bounds"))?;

        checked_range(self.path_table_offset, self.path_table_size, file_len)
            .map_err(|_| DolinError::InvalidFormat("path table range is out of bounds"))?;

        if self.data_offset > file_len {
            return Err(DolinError::InvalidFormat("data offset is out of bounds"));
        }

        if self.entry_count == 0 {
            if self.entry_table_size != 0 || self.path_table_size != 0 {
                return Err(DolinError::InvalidFormat(
                    "empty archive must have zero-sized entry/path tables",
                ));
            }
        }

        Ok(())
    }

    pub fn to_bytes(&self) -> [u8; DON_HEADER_SIZE as usize] {
        let mut buf = [0u8; DON_HEADER_SIZE as usize];

        buf[0..4].copy_from_slice(&DON_MAGIC);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..12].copy_from_slice(&self.entry_count.to_le_bytes());

        buf[12..16].copy_from_slice(&0u32.to_le_bytes());

        buf[16..24].copy_from_slice(&self.entry_table_offset.to_le_bytes());
        buf[24..32].copy_from_slice(&self.entry_table_size.to_le_bytes());
        buf[32..40].copy_from_slice(&self.path_table_offset.to_le_bytes());
        buf[40..48].copy_from_slice(&self.path_table_size.to_le_bytes());
        buf[48..56].copy_from_slice(&self.data_offset.to_le_bytes());
        buf[56..60].copy_from_slice(&self.archive_crc32.to_le_bytes());

        buf[60..64].copy_from_slice(&0u32.to_le_bytes());

        // v3: chunk_table_offset (byte 64..72) + chunk_table_size (byte 72..80)
        buf[64..72].copy_from_slice(&self.chunk_table_offset.to_le_bytes());
        buf[72..80].copy_from_slice(&self.chunk_table_size.to_le_bytes());

        // reserved sisanya (80..96) fill 0
        buf[80..(64 + RESERVED_LEN)].fill(0);

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < DON_HEADER_SIZE as usize {
            return Err(DolinError::InvalidFormat("header too small"));
        }

        if bytes[0..4] != DON_MAGIC {
            return Err(DolinError::InvalidFormat("invalid DON magic"));
        }

        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
        let entry_count = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);

        let entry_table_offset = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        let entry_table_size = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        let path_table_offset = u64::from_le_bytes([
            bytes[32], bytes[33], bytes[34], bytes[35], bytes[36], bytes[37], bytes[38], bytes[39],
        ]);

        let path_table_size = u64::from_le_bytes([
            bytes[40], bytes[41], bytes[42], bytes[43], bytes[44], bytes[45], bytes[46], bytes[47],
        ]);

        let data_offset = u64::from_le_bytes([
            bytes[48], bytes[49], bytes[50], bytes[51], bytes[52], bytes[53], bytes[54], bytes[55],
        ]);

        let archive_crc32 = u32::from_le_bytes([bytes[56], bytes[57], bytes[58], bytes[59]]);

        // v3: chunk_table_offset (byte 64..72) + chunk_table_size (byte 72..80)
        // Kalau 0, berarti v1/v2 (tidak ada chunk table)
        let chunk_table_offset = u64::from_le_bytes([
            bytes[64], bytes[65], bytes[66], bytes[67],
            bytes[68], bytes[69], bytes[70], bytes[71],
        ]);
        let chunk_table_size = u64::from_le_bytes([
            bytes[72], bytes[73], bytes[74], bytes[75],
            bytes[76], bytes[77], bytes[78], bytes[79],
        ]);

        Ok(Self {
            version,
            flags,
            entry_count,
            entry_table_offset,
            entry_table_size,
            path_table_offset,
            path_table_size,
            data_offset,
            archive_crc32,
            chunk_table_offset,
            chunk_table_size,
        })
    }

    pub fn read_from<R: Read>(reader: &mut R) -> Result<Self> {
        let mut buf = [0u8; DON_HEADER_SIZE as usize];
        reader.read_exact(&mut buf)?;
        Self::from_bytes(&buf)
    }

    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<()> {
        let buf = self.to_bytes();
        writer.write_all(&buf)?;
        Ok(())
    }
}

pub fn checked_range(offset: u64, size: u64, file_len: u64) -> std::result::Result<(), ()> {
    let end = offset.checked_add(size).ok_or(())?;
    if end > file_len {
        return Err(());
    }
    Ok(())
}

pub fn is_don_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0..4] == DON_MAGIC
}

pub fn read_header_from_slice(bytes: &[u8]) -> Result<DonHeader> {
    DonHeader::from_bytes(bytes)
}

pub fn default_header() -> DonHeader {
    DonHeader {
        version: DON_VERSION,
        flags: 0,
        entry_count: 0,
        entry_table_offset: DON_HEADER_SIZE,
        entry_table_size: 0,
        path_table_offset: DON_HEADER_SIZE,
        path_table_size: 0,
        data_offset: DON_HEADER_SIZE,
        archive_crc32: 0,
        chunk_table_offset: 0,
        chunk_table_size: 0,
    }
}
