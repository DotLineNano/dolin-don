use std::path::Path;

use crate::error::Result;
use crate::format::reader::read_archive;
use crate::types::DonMethod;

#[derive(Debug, Clone, Default)]
pub struct ArchiveInfo {
    pub entry_count: usize,
    pub stored_size: u64,
    pub original_size: u64,
    pub store_count: usize,
    pub rle_count: usize,
    pub lz_count: usize,
}

impl ArchiveInfo {
    pub fn saved_bytes(&self) -> u64 {
        self.original_size.saturating_sub(self.stored_size)
    }

    pub fn saved_percent(&self) -> f64 {
        if self.original_size == 0 {
            0.0
        } else {
            (self.saved_bytes() as f64 * 100.0) / self.original_size as f64
        }
    }
}

pub fn archive_info(path: &Path) -> Result<ArchiveInfo> {
    let entries = read_archive(path)?;

    let mut info = ArchiveInfo {
        entry_count: entries.len(),
        ..ArchiveInfo::default()
    };

    for entry in &entries {
        info.stored_size += entry.stored_size;
        info.original_size += entry.original_size;

        match entry.method {
            DonMethod::Stored => info.store_count += 1,
            DonMethod::Rle => info.rle_count += 1,
            DonMethod::Lz => info.lz_count += 1,
            DonMethod::LzV5 => info.lz_count += 1,
        }
    }

    Ok(info)
}
