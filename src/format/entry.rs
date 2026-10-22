use crate::error::{DolinError, Result};
use crate::types::{DonEntry, DonMethod, DonRawEntry, DON_ENTRY_FLAG_FILE, DON_ENTRY_SIZE};

impl DonRawEntry {
    pub fn to_bytes(&self) -> [u8; DON_ENTRY_SIZE as usize] {
        let mut buf = [0u8; DON_ENTRY_SIZE as usize];  // 68 byte (v3)

        buf[0..4].copy_from_slice(&self.path_offset.to_le_bytes());
        buf[4..6].copy_from_slice(&self.path_len.to_le_bytes());
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..16].copy_from_slice(&self.data_offset.to_le_bytes());
        buf[16..24].copy_from_slice(&self.stored_size.to_le_bytes());
        buf[24..32].copy_from_slice(&self.original_size.to_le_bytes());
        buf[32..36].copy_from_slice(&self.crc32.to_le_bytes());
        buf[36..38].copy_from_slice(&self.method.to_le_bytes());
        buf[38..40].copy_from_slice(&self.reserved0.to_le_bytes());
        buf[40..48].copy_from_slice(&self.mtime_unix.to_le_bytes());
        buf[48..56].copy_from_slice(&self.dedup_hash.to_le_bytes());
        // v3: chunk_table_offset (byte 56..64) + chunk_count (byte 64..68)
        buf[56..64].copy_from_slice(&self.chunk_table_offset.to_le_bytes());
        buf[64..68].copy_from_slice(&self.chunk_count.to_le_bytes());

        buf
    }

    /// Parse entry v1 (48 byte) atau v2 (56 byte) berdasarkan version.
    pub fn from_bytes_v1(bytes: &[u8], version: u16) -> Result<Self> {
        use crate::types::DON_VERSION_V1;
        
        if version == DON_VERSION_V1 {
            // v1: 48 byte, tanpa dedup_hash
            if bytes.len() < 48 {
                return Err(DolinError::InvalidFormat("entry v1 record too small"));
            }
            let path_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            let path_len = u16::from_le_bytes([bytes[4], bytes[5]]);
            let flags = u16::from_le_bytes([bytes[6], bytes[7]]);
            let data_offset = u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]);
            let stored_size = u64::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23]]);
            let original_size = u64::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31]]);
            let crc32 = u32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);
            let method = u16::from_le_bytes([bytes[36], bytes[37]]);
            let reserved0 = u16::from_le_bytes([bytes[38], bytes[39]]);
            let mtime_unix = u64::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43], bytes[44], bytes[45], bytes[46], bytes[47]]);
            Ok(Self {
                path_offset,
                path_len,
                flags,
                data_offset,
                stored_size,
                original_size,
                crc32,
                method,
                reserved0,
                mtime_unix,
                dedup_hash: 0,
                chunk_table_offset: 0,
                chunk_count: 0,
            })
        } else {
            // v2: 56 byte, dengan dedup_hash
            Self::from_bytes(bytes)
        }
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < DON_ENTRY_SIZE as usize {
            return Err(DolinError::InvalidFormat("entry record too small"));
        }

        let path_offset = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let path_len = u16::from_le_bytes([bytes[4], bytes[5]]);
        let flags = u16::from_le_bytes([bytes[6], bytes[7]]);

        let data_offset = u64::from_le_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]);

        let stored_size = u64::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23],
        ]);

        let original_size = u64::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31],
        ]);

        let crc32 = u32::from_le_bytes([bytes[32], bytes[33], bytes[34], bytes[35]]);
        let method = u16::from_le_bytes([bytes[36], bytes[37]]);
        let reserved0 = u16::from_le_bytes([bytes[38], bytes[39]]);

        let mtime_unix = u64::from_le_bytes([
            bytes[40], bytes[41], bytes[42], bytes[43], bytes[44], bytes[45], bytes[46], bytes[47],
        ]);
        let dedup_hash = u64::from_le_bytes([
            bytes[48], bytes[49], bytes[50], bytes[51], bytes[52], bytes[53], bytes[54], bytes[55],
        ]);

        // v3: chunk_table_offset + chunk_count (kalau bytes.len() >= 68)
        let (chunk_table_offset, chunk_count) = if bytes.len() >= 68 {
            let off = u64::from_le_bytes([
                bytes[56], bytes[57], bytes[58], bytes[59],
                bytes[60], bytes[61], bytes[62], bytes[63],
            ]);
            let cnt = u32::from_le_bytes([bytes[64], bytes[65], bytes[66], bytes[67]]);
            (off, cnt)
        } else {
            (0, 0)
        };

        Ok(Self {
            path_offset,
            path_len,
            flags,
            data_offset,
            stored_size,
            original_size,
            crc32,
            method,
            reserved0,
            mtime_unix,
            dedup_hash,
            chunk_table_offset,
            chunk_count,
        })
    }

    pub fn validate_basic(&self) -> Result<()> {
        if self.path_len == 0 {
            return Err(DolinError::InvalidFormat("entry path length is zero"));
        }

        if self.flags & DON_ENTRY_FLAG_FILE == 0 {
            return Err(DolinError::InvalidFormat("entry is missing file flag"));
        }

        if DonMethod::from_u16(self.method).is_none() {
            return Err(DolinError::InvalidFormat("entry uses unsupported method"));
        }

        Ok(())
    }

    pub fn to_entry(&self, path: String) -> Result<DonEntry> {
        self.validate_basic()?;

        let method = DonMethod::from_u16(self.method)
            .ok_or(DolinError::InvalidFormat("unsupported entry method"))?;

        Ok(DonEntry {
            path,
            flags: self.flags,
            data_offset: self.data_offset,
            stored_size: self.stored_size,
            original_size: self.original_size,
            crc32: self.crc32,
            method,
            mtime_unix: self.mtime_unix,
            dedup_hash: self.dedup_hash,
            dedup_source: None,
            chunk_table_offset: self.chunk_table_offset,
            chunk_count: self.chunk_count,
        })
    }
}

impl DonEntry {
    pub fn to_raw(&self, path_offset: u32, path_len: u16) -> DonRawEntry {
        DonRawEntry {
            path_offset,
            path_len,
            flags: self.flags,
            data_offset: self.data_offset,
            stored_size: self.stored_size,
            original_size: self.original_size,
            crc32: self.crc32,
            method: self.method.to_u16(),
            reserved0: 0,
            mtime_unix: self.mtime_unix,
            dedup_hash: self.dedup_hash,
            chunk_table_offset: self.chunk_table_offset,
            chunk_count: self.chunk_count,
        }
    }
}

pub fn decode_raw_entries(bytes: &[u8], entry_count: u32, version: u16) -> Result<Vec<DonRawEntry>> {
    use crate::types::{DON_ENTRY_SIZE_V1, DON_VERSION_V1};
    
    let entry_size = if version == DON_VERSION_V1 {
        DON_ENTRY_SIZE_V1 as usize
    } else {
        DON_ENTRY_SIZE as usize
    };
    
    let expected_size = (entry_count as usize)
        .checked_mul(entry_size)
        .ok_or(DolinError::InvalidFormat("entry table size overflow"))?;

    if bytes.len() < expected_size {
        return Err(DolinError::InvalidFormat("entry table is truncated"));
    }

    let mut entries = Vec::with_capacity(entry_count as usize);

    for i in 0..entry_count as usize {
        let start = i * entry_size;
        let end = start + entry_size;
        let raw = DonRawEntry::from_bytes_v1(&bytes[start..end], version)?;
        entries.push(raw);
    }

    Ok(entries)
}

pub fn encode_raw_entries(entries: &[DonRawEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entries.len() * DON_ENTRY_SIZE as usize);
    for entry in entries {
        out.extend_from_slice(&entry.to_bytes());
    }
    out
}

pub fn decode_path_from_table(
    path_table: &[u8],
    path_offset: u32,
    path_len: u16,
) -> Result<String> {
    let start = path_offset as usize;
    let len = path_len as usize;
    let end = start
        .checked_add(len)
        .ok_or(DolinError::InvalidFormat("path range overflow"))?;

    if end > path_table.len() {
        return Err(DolinError::InvalidFormat("path range is out of bounds"));
    }

    let slice = &path_table[start..end];
    let path = std::str::from_utf8(slice)
        .map_err(|_| DolinError::InvalidFormat("path is not valid UTF-8"))?;

    if path.is_empty() {
        return Err(DolinError::InvalidFormat("path is empty"));
    }

    Ok(path.to_string())
}

pub fn build_path_table(entries: &[DonEntry]) -> Result<(Vec<DonRawEntry>, Vec<u8>)> {
    let mut raw_entries = Vec::with_capacity(entries.len());
    let mut path_table = Vec::new();

    for entry in entries {
        let path_bytes = entry.path.as_bytes();

        if path_bytes.is_empty() {
            return Err(DolinError::InvalidFormat("entry path is empty"));
        }

        if path_bytes.len() > u16::MAX as usize {
            return Err(DolinError::InvalidFormat("entry path is too long"));
        }

        if path_table.len() > u32::MAX as usize {
            return Err(DolinError::InvalidFormat("path table is too large"));
        }

        let path_offset = path_table.len() as u32;
        let path_len = path_bytes.len() as u16;

        path_table.extend_from_slice(path_bytes);
        raw_entries.push(entry.to_raw(path_offset, path_len));
    }

    Ok((raw_entries, path_table))
}

pub fn new_file_entry(
    path: String,
    data_offset: u64,
    stored_size: u64,
    original_size: u64,
    crc32: u32,
    method: DonMethod,
    mtime_unix: u64,
) -> DonEntry {
    DonEntry {
        path,
        flags: DON_ENTRY_FLAG_FILE,
        data_offset,
        stored_size,
        original_size,
        crc32,
        method,
        mtime_unix,
        dedup_hash: 0,
        dedup_source: None,
        chunk_table_offset: 0,
        chunk_count: 0,
    }
}
