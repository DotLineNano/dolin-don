use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crc32fast::Hasher;

use crate::api::DonEntryInfo;
use crate::don_lz;
use crate::don_rle;
use crate::error::{DolinError, Result};
use crate::format::entry::{decode_path_from_table, decode_raw_entries};
use crate::types::{DonEntry, DonHeader, DonMethod, DON_ENTRY_SIZE, DON_HEADER_SIZE};
use crate::util::sanitize_relative_path;

pub fn read_archive_entries(archive_path: &Path) -> Result<Vec<DonEntryInfo>> {
    let entries = read_archive(archive_path)?;
    Ok(entries.into_iter().map(to_entry_info).collect())
}

pub fn extract_archive_to_dir(archive_path: &Path, output_dir: &Path) -> Result<()> {
    let entries = read_archive(archive_path)?;
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

    let expected_entry_table_size = (header.entry_count as u64)
        .checked_mul(DON_ENTRY_SIZE)
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

    let raw_entries = decode_raw_entries(&entry_table, header.entry_count)?;
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

        let entry = raw.to_entry(path)?;
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
        archive_file.seek(SeekFrom::Start(entry.data_offset))?;

        let mut stored = vec![0u8; entry.stored_size as usize];
        archive_file.read_exact(&mut stored)?;

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
        DonMethod::Lz => don_lz::decompress(stored, entry.original_size),
        DonMethod::LzV5 => don_lz::decompress_lz_v5(stored, entry.original_size),
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
