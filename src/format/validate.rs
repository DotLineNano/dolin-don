use std::collections::HashSet;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use crc32fast::Hasher;

use crate::don_lz;
use crate::don_rle;
use crate::error::{DolinError, Result};
use crate::format::entry::{decode_path_from_table, decode_raw_entries};
use crate::types::{DonHeader, DonMethod, DON_ENTRY_SIZE, DON_ENTRY_SIZE_V1, DON_ENTRY_FLAG_DEDUP, DON_HEADER_SIZE, DON_VERSION_V1};
use crate::util::sanitize_relative_path;

pub fn validate_archive(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();

    if file_len < DON_HEADER_SIZE {
        return Err(DolinError::InvalidFormat("archive smaller than header"));
    }

    let header = DonHeader::read_from(&mut file)?;
    header.validate_basic(file_len)?;

    use crate::types::{DON_ENTRY_SIZE_V2, DON_VERSION_V2};
    let entry_size = if header.version == DON_VERSION_V1 {
        DON_ENTRY_SIZE_V1
    } else if header.version == DON_VERSION_V2 {
        DON_ENTRY_SIZE_V2
    } else {
        DON_ENTRY_SIZE  // v3 (68)
    };
    let expected_entry_table_size = (header.entry_count as u64)
        .checked_mul(entry_size)
        .ok_or(DolinError::InvalidFormat("entry table size overflow"))?;

    if header.entry_table_size != expected_entry_table_size {
        return Err(DolinError::InvalidFormat("entry table size mismatch"));
    }

    file.seek(SeekFrom::Start(header.entry_table_offset))?;
    let mut entry_table = vec![0u8; header.entry_table_size as usize];
    file.read_exact(&mut entry_table)?;

    file.seek(SeekFrom::Start(header.path_table_offset))?;
    let mut path_table = vec![0u8; header.path_table_size as usize];
    file.read_exact(&mut path_table)?;

    let raw_entries = decode_raw_entries(&entry_table, header.entry_count, header.version)?;

    let mut seen_paths = HashSet::new();
    let mut data_ranges = Vec::with_capacity(raw_entries.len());

    for raw in &raw_entries {
        raw.validate_basic()?;

        let decoded_path = decode_path_from_table(&path_table, raw.path_offset, raw.path_len)?;
        let safe_path = sanitize_relative_path(Path::new(&decoded_path))?;

        if !seen_paths.insert(safe_path.clone()) {
            return Err(DolinError::InvalidFormat("duplicate entry path"));
        }

        if raw.data_offset < header.data_offset {
            return Err(DolinError::InvalidFormat(
                "entry data offset is before data section",
            ));
        }

        let data_end = raw
            .data_offset
            .checked_add(raw.stored_size)
            .ok_or(DolinError::InvalidFormat("entry data range overflow"))?;

        if data_end > file_len {
            return Err(DolinError::InvalidFormat("entry data is out of bounds"));
        }

        // Skip overlap check untuk entry dedup (data_offset menunjuk ke entry pertama)
        if raw.flags & DON_ENTRY_FLAG_DEDUP == 0 {
            data_ranges.push((raw.data_offset, data_end, safe_path));
        }
    }

    data_ranges.sort_by_key(|(start, _, _)| *start);

    for window in data_ranges.windows(2) {
        let (prev_start, prev_end, _prev_path) = &window[0];
        let (next_start, _next_end, _next_path) = &window[1];

        if next_start < prev_start {
            return Err(DolinError::InvalidFormat("entry data ranges are unordered"));
        }

        if next_start < prev_end {
            return Err(DolinError::InvalidFormat("entry data ranges overlap"));
        }
    }

    for raw in &raw_entries {
        file.seek(SeekFrom::Start(raw.data_offset))?;
        let mut stored = vec![0u8; raw.stored_size as usize];
        file.read_exact(&mut stored)?;

        let decoded = match DonMethod::from_u16(raw.method) {
            Some(DonMethod::Stored) => stored,
            Some(DonMethod::Rle) => don_rle::decompress(&stored, raw.original_size)?,
            Some(DonMethod::RleExt) => don_rle::decompress_ext(&stored, raw.original_size)?,
            Some(DonMethod::Lz) => don_lz::decompress(&stored, raw.original_size)?,
            Some(DonMethod::LzV5) => don_lz::decompress_lz_v5(&stored, raw.original_size)?,
            Some(DonMethod::LzV5Huff) => don_lz::decompress_lz_v5_huff(&stored, raw.original_size)?,
            Some(DonMethod::Deflate) => miniz_oxide::inflate::decompress_to_vec(&stored)
                .map_err(|e| DolinError::Format(format!("deflate: {:?}", e)))?,
            None => return Err(DolinError::InvalidFormat("entry uses unsupported method")),
        };

        if decoded.len() as u64 != raw.original_size {
            return Err(DolinError::InvalidFormat("decoded size mismatch"));
        }

        let mut hasher = Hasher::new();
        hasher.update(&decoded);
        let actual_crc32 = hasher.finalize();

        if actual_crc32 != raw.crc32 {
            return Err(DolinError::InvalidFormat("entry crc32 mismatch"));
        }
    }

    Ok(())
}
