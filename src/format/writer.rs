use std::fs::{self, File};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::don_lz;
use crate::don_rle;
use crate::error::{DolinError, Result};
use crate::format::entry::{build_path_table, encode_raw_entries, new_file_entry};
use crate::format::header::default_header;
use crate::types::{DonEntry, DonMethod, DON_ENTRY_SIZE, DON_HEADER_SIZE};
use crate::util::{crc32_file, sanitize_relative_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackMethod {
    Auto,
    Store,
    Rle,
    Lz,
    LzV5,
}

impl PackMethod {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "store" | "stored" => Some(Self::Store),
            "rle" => Some(Self::Rle),
            "lz" => Some(Self::Lz),
            "lzv5" | "lz-v5" | "v5" => Some(Self::LzV5),
            _ => None,
        }
    }
}

pub fn pack_path_to_archive(input_path: &Path, output_file: &Path) -> Result<()> {
    pack_path_to_archive_with_method(input_path, output_file, PackMethod::Auto)
}

pub fn pack_path_to_archive_with_method(
    input_path: &Path,
    output_file: &Path,
    pack_method: PackMethod,
) -> Result<()> {
    let input_path = input_path.canonicalize()?;
    let output_abs = absolutize_for_output(output_file)?;

    if output_abs.starts_with(&input_path) {
        return Err(DolinError::InvalidPath(output_file.display().to_string()));
    }

    let collected = collect_entries(&input_path)?;
    let prepared = prepare_entries(&collected, pack_method)?;
    let mut entries: Vec<DonEntry> = prepared.iter().map(|item| item.entry.clone()).collect();

    let (initial_raw_entries, path_table) = build_path_table(&entries)?;

    let entry_table_offset = DON_HEADER_SIZE;
    let entry_table_size = (initial_raw_entries.len() as u64)
        .checked_mul(DON_ENTRY_SIZE)
        .ok_or(DolinError::InvalidFormat("entry table size overflow"))?;

    let path_table_offset = entry_table_offset
        .checked_add(entry_table_size)
        .ok_or(DolinError::InvalidFormat("path table offset overflow"))?;

    let path_table_size = path_table.len() as u64;

    let data_offset = path_table_offset
        .checked_add(path_table_size)
        .ok_or(DolinError::InvalidFormat("data offset overflow"))?;

    assign_data_offsets(&mut entries, data_offset)?;

    for (entry, prepared_item) in entries.iter_mut().zip(prepared.iter()) {
        entry.stored_size = prepared_item.payload.len() as u64;
        entry.original_size = prepared_item.original_size;
        entry.crc32 = prepared_item.crc32;
        entry.method = prepared_item.method;
        entry.mtime_unix = prepared_item.mtime_unix;
    }

    let (raw_entries, path_table) = build_path_table(&entries)?;
    let entry_table_bytes = encode_raw_entries(&raw_entries);

    let mut header = default_header();
    header.entry_count = entries.len() as u32;
    header.entry_table_offset = entry_table_offset;
    header.entry_table_size = entry_table_bytes.len() as u64;
    header.path_table_offset = path_table_offset;
    header.path_table_size = path_table.len() as u64;
    header.data_offset = data_offset;
    header.archive_crc32 = 0;

    let parent = output_file.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

    header.write_to(&mut writer)?;
    writer.write_all(&entry_table_bytes)?;
    writer.write_all(&path_table)?;

    for item in &prepared {
        writer.write_all(&item.payload)?;
    }

    writer.flush()?;

    let mut inner = writer
        .into_inner()
        .map_err(|e| DolinError::Io(e.into_error()))?;
    inner.seek(SeekFrom::Start(0))?;
    header.write_to(&mut inner)?;
    inner.flush()?;

    Ok(())
}

#[derive(Debug, Clone)]
struct CollectedEntry {
    rel_path: String,
    source_path: PathBuf,
    mtime_unix: u64,
    crc32: u32,
}

#[derive(Debug, Clone)]
struct PreparedEntry {
    entry: DonEntry,
    payload: Vec<u8>,
    original_size: u64,
    crc32: u32,
    method: DonMethod,
    mtime_unix: u64,
}

fn collect_entries(input_path: &Path) -> Result<Vec<CollectedEntry>> {
    if input_path.is_file() {
        let metadata = fs::metadata(input_path)?;
        let file_name = input_path
            .file_name()
            .ok_or(DolinError::InvalidPath(input_path.display().to_string()))?;
        let rel = sanitize_relative_path(Path::new(file_name))?;
        let mtime_unix = modified_to_unix(&metadata)?;
        let crc32 = crc32_file(input_path)?;

        return Ok(vec![CollectedEntry {
            rel_path: rel,
            source_path: input_path.to_path_buf(),
            mtime_unix,
            crc32,
        }]);
    }

    if !input_path.is_dir() {
        return Err(DolinError::InvalidPath(input_path.display().to_string()));
    }

    let mut out = Vec::new();
    collect_dir_recursive(input_path, input_path, &mut out)?;
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    Ok(out)
}

fn collect_dir_recursive(root: &Path, current: &Path, out: &mut Vec<CollectedEntry>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            collect_dir_recursive(root, &path, out)?;
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let rel_path = path
            .strip_prefix(root)
            .map_err(|_| DolinError::InvalidPath(path.display().to_string()))?;
        let rel = sanitize_relative_path(rel_path)?;
        let mtime_unix = modified_to_unix(&metadata)?;
        let crc32 = crc32_file(&path)?;

        out.push(CollectedEntry {
            rel_path: rel,
            source_path: path,
            mtime_unix,
            crc32,
        });
    }

    Ok(())
}

fn prepare_entries(
    items: &[CollectedEntry],
    pack_method: PackMethod,
) -> Result<Vec<PreparedEntry>> {
    let mut entries = Vec::with_capacity(items.len());

    for item in items {
        let original = fs::read(&item.source_path)?;

        let (payload, method) = match pack_method {
            PackMethod::Store => (original.clone(), DonMethod::Stored),
            PackMethod::Rle => (don_rle::compress(&original), DonMethod::Rle),
            PackMethod::Lz => (don_lz::compress(&original), DonMethod::Lz),
            PackMethod::LzV5 => (don_lz::compress_lz_v5(&original), DonMethod::LzV5),
            PackMethod::Auto => {
                let rle = don_rle::compress(&original);
                let lz = don_lz::compress(&original);
                let lzv5 = don_lz::compress_lz_v5(&original);

                let mut payload = original.clone();
                let mut method = DonMethod::Stored;

                if rle.len() < payload.len() {
                    payload = rle;
                    method = DonMethod::Rle;
                }

                if lz.len() < payload.len() {
                    payload = lz;
                    method = DonMethod::Lz;
                }

                if lzv5.len() < payload.len() {
                    payload = lzv5;
                    method = DonMethod::LzV5;
                }

                (payload, method)
            }
        };

        let entry = new_file_entry(
            item.rel_path.clone(),
            0,
            payload.len() as u64,
            original.len() as u64,
            item.crc32,
            method,
            item.mtime_unix,
        );

        entries.push(PreparedEntry {
            entry,
            payload,
            original_size: original.len() as u64,
            crc32: item.crc32,
            method,
            mtime_unix: item.mtime_unix,
        });
    }

    Ok(entries)
}

fn assign_data_offsets(entries: &mut [DonEntry], start_offset: u64) -> Result<()> {
    let mut offset = start_offset;

    for entry in entries {
        entry.data_offset = offset;
        offset = offset
            .checked_add(entry.stored_size)
            .ok_or(DolinError::InvalidFormat("data offset overflow"))?;
    }

    Ok(())
}

fn absolutize_for_output(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    Ok(std::env::current_dir()?.join(path))
}

fn modified_to_unix(metadata: &fs::Metadata) -> Result<u64> {
    let modified = metadata.modified()?;
    let duration = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| DolinError::InvalidFormat("file modified time is before unix epoch"))?;
    Ok(duration.as_secs())
}
