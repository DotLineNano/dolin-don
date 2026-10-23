use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crc32fast::Hasher;

use crate::api::DonEntryInfo;
use crate::don_lz;
use crate::don_rle;
use crate::error::{DolinError, Result};
use crate::format::entry::{decode_path_from_table, decode_raw_entries};
use crate::types::{
    DonEntry, DonHeader, DonMethod, DON_ENTRY_SIZE, DON_ENTRY_SIZE_V1,
    DON_HEADER_SIZE, DON_VERSION_V1,
};
use crate::util::sanitize_relative_path;

pub fn read_archive_entries(archive_path: &Path) -> Result<Vec<DonEntryInfo>> {
    let entries = read_archive(archive_path)?;
    Ok(entries.into_iter().map(to_entry_info).collect())
}

/// Options for extract operations.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExtractOptions {
    /// Replace output dir even if non-empty (legacy behavior).
    pub overwrite: bool,
    /// Merge into existing dir instead of replacing (tar/unzip style).
    pub merge: bool,
}

pub fn extract_archive_to_dir(archive_path: &Path, output_dir: &Path) -> Result<()> {
    extract_archive_to_dir_with(archive_path, output_dir, ExtractOptions::default())
}

pub fn extract_archive_to_dir_with(
    archive_path: &Path,
    output_dir: &Path,
    opts: ExtractOptions,
) -> Result<()> {
    let entries = read_archive(archive_path)?;

    // Pre-check: output_dir must not be an existing non-directory file.
    if output_dir.exists() && !output_dir.is_dir() {
        return Err(DolinError::InvalidPath(format!(
            "{}: exists but is not a directory",
            output_dir.display()
        )));
    }

    // MERGE mode: extract directly into existing dir, overwrite per-file.
    if opts.merge {
        fs::create_dir_all(output_dir)?;
        let mut archive_file = File::open(archive_path)?;
        for entry in entries {
            let safe_rel = sanitize_relative_path(Path::new(&entry.path))?;
            let out_path = output_dir.join(&safe_rel);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let checked_out = ensure_within_base(output_dir, &out_path)?;
            extract_entry_payload_atomic(&mut archive_file, &entry, &checked_out)?;
        }
        return Ok(());
    }

    // SAFE DEFAULT: refuse non-empty output unless --force given.
    if output_dir.exists() && !opts.overwrite {
        let has_content = fs::read_dir(output_dir)?.next().is_some();
        if has_content {
            return Err(DolinError::OutputNotEmpty(output_dir.to_path_buf()));
        }
    }

    // ATOMIC mode (default or --force): extract to staging then swap.
    let staging_dir = temporary_output_dir(output_dir);
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)?;
    }
    fs::create_dir_all(&staging_dir)?;

    let result = (|| -> Result<()> {
        let mut archive_file = File::open(archive_path)?;
        for entry in entries {
            let safe_rel = sanitize_relative_path(Path::new(&entry.path))?;
            let out_path = staging_dir.join(&safe_rel);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let checked_out = ensure_within_base(&staging_dir, &out_path)?;
            extract_entry_payload_atomic(&mut archive_file, &entry, &checked_out)?;
        }
        Ok(())
    })();

    if let Err(err) = result {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(err);
    }

    if output_dir.exists() {
        fs::remove_dir_all(output_dir)?;
    }
    fs::rename(&staging_dir, output_dir)?;
    Ok(())
}

pub fn extract_archive_entry(
    archive_path: &Path,
    entry_path: &str,
    output_file: &Path,
) -> Result<()> {
    let entries = read_archive(archive_path)?;

    let wanted = sanitize_relative_path(Path::new(entry_path))?;
    let entry = entries
        .into_iter()
        .find(|e| e.path == wanted)
        .ok_or_else(|| DolinError::EntryNotFound(entry_path.to_string()))?;

    if let Some(parent) = output_file.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut archive_file = File::open(archive_path)?;
    extract_entry_payload_atomic(&mut archive_file, &entry, output_file)?;
    Ok(())
}

pub fn read_archive(archive_path: &Path) -> Result<Vec<DonEntry>> {
    let mut file = File::open(archive_path)?;
    let file_len = file.metadata()?.len();

    if file_len < DON_HEADER_SIZE {
        return Err(DolinError::InvalidFormat("archive smaller than header"));
    }

    let header = DonHeader::read_from(&mut file)?;
    header.validate_basic(file_len)?;

    // Pilih entry size berdasarkan version
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

    // v3: baca chunk table (kalau ada)
    let _chunk_table: Vec<crate::types::DonChunk> = if header.chunk_table_size > 0 {
        file.seek(SeekFrom::Start(header.chunk_table_offset))?;
        let mut chunk_bytes = vec![0u8; header.chunk_table_size as usize];
        file.read_exact(&mut chunk_bytes)?;
        let count = chunk_bytes.len() / 16;
        let mut chunks = Vec::with_capacity(count);
        for i in 0..count {
            let off = i * 16;
            let offset = u64::from_le_bytes([
                chunk_bytes[off], chunk_bytes[off + 1], chunk_bytes[off + 2], chunk_bytes[off + 3],
                chunk_bytes[off + 4], chunk_bytes[off + 5], chunk_bytes[off + 6], chunk_bytes[off + 7],
            ]);
            let len = u32::from_le_bytes([
                chunk_bytes[off + 8], chunk_bytes[off + 9], chunk_bytes[off + 10], chunk_bytes[off + 11],
            ]);
            let crc = u32::from_le_bytes([
                chunk_bytes[off + 12], chunk_bytes[off + 13], chunk_bytes[off + 14], chunk_bytes[off + 15],
            ]);
            chunks.push(crate::types::DonChunk { offset, len, crc32: crc });
        }
        chunks
    } else {
        Vec::new()
    };

    let raw_entries = decode_raw_entries(&entry_table, header.entry_count, header.version)?;
    let mut entries = Vec::with_capacity(raw_entries.len());

    for raw in raw_entries {
        raw.validate_basic()?;

        let path = decode_path_from_table(&path_table, raw.path_offset, raw.path_len)?;
        let path = sanitize_relative_path(Path::new(&path))?;

        let data_end = raw
            .data_offset
            .checked_add(raw.stored_size)
            .ok_or(DolinError::InvalidFormat("entry data range overflow"))?;

        if raw.data_offset < header.data_offset {
            return Err(DolinError::InvalidFormat(
                "entry data offset is before data section",
            ));
        }

        if data_end > file_len {
            return Err(DolinError::InvalidFormat("entry data is out of bounds"));
        }

        let mut entry = raw.to_entry(path)?;
        // v3: set chunk info dari raw
        entry.chunk_table_offset = raw.chunk_table_offset;
        entry.chunk_count = raw.chunk_count;
        entries.push(entry);
    }

    Ok(entries)
}

fn extract_entry_payload_atomic(
    archive_file: &mut File,
    entry: &DonEntry,
    output_file: &Path,
) -> Result<()> {
    let temp_path = temporary_output_path(output_file);

    if let Some(parent) = temp_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let write_result = (|| -> Result<()> {
        // v3: kalau entry punya chunk, rekonstruksi payload dari chunk
        let stored: Vec<u8> = if entry.chunk_count > 0 {
            // Baca chunk table dari archive (seek ke awal dulu)
            archive_file.seek(SeekFrom::Start(0))?;
            let header = DonHeader::read_from(archive_file)?;
            let chunk_table: Vec<crate::types::DonChunk> = if header.chunk_table_size > 0 {
                archive_file.seek(SeekFrom::Start(header.chunk_table_offset))?;
                let mut chunk_bytes = vec![0u8; header.chunk_table_size as usize];
                archive_file.read_exact(&mut chunk_bytes)?;
                let count = chunk_bytes.len() / 16;
                let mut chunks = Vec::with_capacity(count);
                for i in 0..count {
                    let off = i * 16;
                    let offset = u64::from_le_bytes([
                        chunk_bytes[off], chunk_bytes[off + 1], chunk_bytes[off + 2], chunk_bytes[off + 3],
                        chunk_bytes[off + 4], chunk_bytes[off + 5], chunk_bytes[off + 6], chunk_bytes[off + 7],
                    ]);
                    let len = u32::from_le_bytes([
                        chunk_bytes[off + 8], chunk_bytes[off + 9], chunk_bytes[off + 10], chunk_bytes[off + 11],
                    ]);
                    let crc = u32::from_le_bytes([
                        chunk_bytes[off + 12], chunk_bytes[off + 13], chunk_bytes[off + 14], chunk_bytes[off + 15],
                    ]);
                    chunks.push(crate::types::DonChunk { offset, len, crc32: crc });
                }
                chunks
            } else {
                Vec::new()
            };
            
            // Rekonstruksi: ambil chunk pertama entry (chunk_table_offset)
            // Entry punya chunk_count chunk, mulai dari chunk_table_offset
            let start_chunk_idx = (entry.chunk_table_offset / 16) as usize;
            let mut payload = Vec::new();
            for i in 0..entry.chunk_count as usize {
                let chunk_idx = start_chunk_idx + i;
                if chunk_idx >= chunk_table.len() {
                    return Err(DolinError::InvalidFormat("chunk index out of bounds".into()));
                }
                let chunk = &chunk_table[chunk_idx];
                archive_file.seek(SeekFrom::Start(chunk.offset))?;
                let mut chunk_data = vec![0u8; chunk.len as usize];
                archive_file.read_exact(&mut chunk_data)?;
                payload.extend_from_slice(&chunk_data);
            }
            payload
        } else {
            // v1/v2 atau file kecil: baca payload langsung
            archive_file.seek(SeekFrom::Start(entry.data_offset))?;
            let mut stored = vec![0u8; entry.stored_size as usize];
            archive_file.read_exact(&mut stored)?;
            stored
        };

        // DEBUG: print runtime values

        let decoded = decode_entry_payload(entry, &stored)?;

        if decoded.len() as u64 != entry.original_size {
            return Err(DolinError::InvalidFormat(
                "decoded size mismatch during extract",
            ));
        }

        let mut hasher = Hasher::new();
        hasher.update(&decoded);
        let actual_crc32 = hasher.finalize();

        if actual_crc32 != entry.crc32 {
            return Err(DolinError::InvalidFormat(
                "entry crc32 mismatch during extract",
            ));
        }

        let mut out = File::create(&temp_path)?;
        out.write_all(&decoded)?;
        out.flush()?;

        // Restore mtime dari entry (kalau ada)
        if entry.mtime_unix > 0 {
            let mtime = std::time::UNIX_EPOCH
                + std::time::Duration::from_secs(entry.mtime_unix);
            let times = std::fs::FileTimes::new().set_modified(mtime);
            let _ = out.set_times(times);
        }

        drop(out);

        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    fs::rename(&temp_path, output_file)?;
    Ok(())
}

fn decode_entry_payload(entry: &DonEntry, stored: &[u8]) -> Result<Vec<u8>> {
    match entry.method {
        DonMethod::Stored => Ok(stored.to_vec()),
        DonMethod::Rle => don_rle::decompress(stored, entry.original_size),
        DonMethod::RleExt => don_rle::decompress_ext(stored, entry.original_size),
        DonMethod::Lz => don_lz::decompress(stored, entry.original_size),
        DonMethod::LzV5 => don_lz::decompress_lz_v5(stored, entry.original_size),
        DonMethod::LzV5Huff => don_lz::decompress_lz_v5_huff(stored, entry.original_size),
        DonMethod::Deflate => miniz_oxide::inflate::decompress_to_vec(stored)
            .map_err(|e| DolinError::Format(format!("deflate: {:?}", e))),
    }
}

fn temporary_output_path(output_file: &Path) -> PathBuf {
    let file_name = output_file
        .file_name()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());

    let temp_name = format!(".{}.part", file_name);
    output_file.with_file_name(temp_name)
}

fn temporary_output_dir(output_dir: &Path) -> PathBuf {
    let dir_name = output_dir
        .file_name()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".to_string());

    let temp_name = format!(".{}.partial", dir_name);
    output_dir.with_file_name(temp_name)
}

fn to_entry_info(entry: DonEntry) -> DonEntryInfo {
    DonEntryInfo {
        path: entry.path,
        stored_size: entry.stored_size,
        original_size: entry.original_size,
        crc32: entry.crc32,
        method: entry.method.to_u16(),
        mtime_unix: entry.mtime_unix,
    }
}

fn ensure_within_base(base: &Path, target: &Path) -> Result<PathBuf> {
    let base_abs = if base.exists() {
        base.canonicalize()?
    } else {
        let parent = base.parent().unwrap_or_else(|| Path::new("."));
        let parent_abs = parent.canonicalize()?;
        parent_abs.join(
            base.file_name()
                .ok_or_else(|| DolinError::InvalidPath(base.display().to_string()))?,
        )
    };

    let target_abs = normalize_joined_path(target)?;

    if !target_abs.starts_with(&base_abs) {
        return Err(DolinError::InvalidPath(target.display().to_string()));
    }

    Ok(target_abs)
}

fn normalize_joined_path(path: &Path) -> Result<PathBuf> {
    let mut out = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir()?
    };

    for comp in path.components() {
        match comp {
            std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                out.push(comp.as_os_str());
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !out.pop() {
                    return Err(DolinError::InvalidPath(path.display().to_string()));
                }
            }
            std::path::Component::Normal(part) => out.push(part),
        }
    }

    Ok(out)
}
