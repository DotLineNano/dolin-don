use std::fs::{self, File};
use std::collections::HashMap;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::don_lz;
use crate::don_rle;
use crate::error::{DolinError, Result};
use crate::format::entry::{build_path_table, encode_raw_entries, new_file_entry};
use crate::format::header::default_header;
use crate::types::{
    DonEntry,
    DonMethod,
    DON_ENTRY_SIZE,
    DON_HEADER_SIZE,
    DON_ENTRY_FLAG_DEDUP,
};
use crate::util::{crc32_file, sanitize_relative_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackMethod {
    Auto,
    Store,
    Rle,
    Lz,
    LzV5,
    LzV5Huff,
    RleExt,
    Deflate,
}

impl PackMethod {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "store" | "stored" => Some(Self::Store),
            "rle" => Some(Self::Rle),
            "lz" => Some(Self::Lz),
            "lzv5" | "lz-v5" | "v5" => Some(Self::LzV5),
            "lzv5huff" | "lz-v5-huff" | "huff" => Some(Self::LzV5Huff),
            "rleext" | "rle-ext" => Some(Self::RleExt),
            "deflate" | "df" => Some(Self::Deflate),
            _ => None,
        }
    }
}

pub fn pack_path_to_archive(input_path: &Path, output_file: &Path) -> Result<()> {
    pack_path_to_archive_with_method(input_path, output_file, PackMethod::Auto)
}

fn is_precompressed_by_ext(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".mp4") || lower.ends_with(".mkv") || lower.ends_with(".avi")
        || lower.ends_with(".apk") || lower.ends_with(".zip") || lower.ends_with(".jar")
        || lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg")
        || lower.ends_with(".gif") || lower.ends_with(".webp")
        || lower.ends_with(".gz") || lower.ends_with(".zst") || lower.ends_with(".xz")
        || lower.ends_with(".7z") || lower.ends_with(".rar")
}

fn is_binary_img(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".img") || lower.ends_with(".bin") || lower.ends_with(".iso")
}

fn detect_file_type(data: &[u8], path: &str) -> &'static str {
    // 1. Magic number (prioritas)
    if data.len() >= 12 {
        if &data[4..8] == b"ftyp" {
            return "precompressed";
        }
        if &data[..4] == b"PK\x03\x04" {
            return "precompressed";
        }
        if &data[..4] == b"\x89PNG" {
            return "precompressed";
        }
        if &data[..3] == b"\xFF\xD8\xFF" {
            return "precompressed";
        }
        if &data[..2] == b"\x1F\x8B" {
            return "precompressed";
        }
        if &data[..4] == b"\x28\xB5\x2F\xFD" {
            return "precompressed";
        }
        if &data[..4] == b"\x7FELF" {
            return "elf";
        }
        if data.len() >= 8 && &data[..8] == b"ANDROID!" {
            return "binary_img";
        }
        if &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            return "precompressed";
        }
    }

    // 2. Cek teks
    let is_text = !data.is_empty() && data.iter().take(1024).all(|&b| {
        b == b'\n' || b == b'\r' || b == b'\t' || (b >= 32 && b < 127)
    });
    if is_text {
        return "text";
    }

    // 3. Fallback ekstensi
    if is_precompressed_by_ext(path) {
        return "precompressed";
    }
    if is_binary_img(path) {
        return "binary_img";
    }

    "generic"
}

fn estimate_entropy(data: &[u8]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sample_size = data.len().min(64 * 1024);
    let sample = &data[..sample_size];
    let mut freq = [0u64; 256];
    for &b in sample {
        freq[b as usize] += 1;
    }
    let len = sample.len() as f64;
    let mut entropy = 0.0f64;
    for &count in &freq {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy as f32
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

    // ═══════════════════════════════════════════════════════
    // DEDUP: deteksi file duplikat (crc32 sama)
    // ═══════════════════════════════════════════════════════
    // Map: crc32 → index entry pertama
    let mut dedup_map: HashMap<u32, usize> = HashMap::new();
    // Set: index yang dedup (skip payload write)
    let mut dedup_indices: Vec<usize> = Vec::new();
    
    for (idx, entry) in entries.iter_mut().enumerate() {
        let crc = entry.crc32;
        
        if crc == 0 {
            // crc32 = 0 artinya belum dihitung, skip dedup
            continue;
        }
        
        if let Some(&first_idx) = dedup_map.get(&crc) {
            // Duplikat: tandai DEDUP, data_offset akan di-set ke first_idx nanti
            entry.flags |= DON_ENTRY_FLAG_DEDUP;
            entry.dedup_source = Some(first_idx);
            dedup_indices.push(idx);
        } else {
            // Pertama: simpan di map
            dedup_map.insert(crc, idx);
        }
    }

    // Build path table (dengan flags DEDUP sudah di-set)
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

    // Set stored_size + original_size + crc32 + method + mtime dari prepared
    // (lakukan SEBELUM assign data_offset, karena dedup butuh stored_size)
    for (entry, prepared_item) in entries.iter_mut().zip(prepared.iter()) {
        entry.stored_size = prepared_item.payload.len() as u64;
        entry.original_size = prepared_item.original_size;
        entry.crc32 = prepared_item.crc32;
        entry.method = prepared_item.method;
        entry.mtime_unix = prepared_item.mtime_unix;
    }

    // Clone data_offset + stored_size untuk dedup (hindari borrow checker)
    // Index → (data_offset, stored_size)
    let mut data_offsets: Vec<u64> = vec![0; entries.len()];
    let mut stored_sizes: Vec<u64> = entries.iter().map(|e| e.stored_size).collect();
    
    let mut current_offset = data_offset;
    for (idx, entry) in entries.iter_mut().enumerate() {
        if dedup_indices.contains(&idx) {
            // Dedup: data_offset menunjuk ke entry pertama
            if let Some(first_idx) = entry.dedup_source {
                entry.data_offset = data_offsets[first_idx];
                entry.stored_size = stored_sizes[first_idx];
            }
            data_offsets[idx] = entry.data_offset;
            stored_sizes[idx] = entry.stored_size;
        } else {
            entry.data_offset = current_offset;
            data_offsets[idx] = current_offset;
            // stored_size sudah di-set di atas dari prepared
            current_offset = current_offset
                .checked_add(entry.stored_size)
                .ok_or(DolinError::InvalidFormat("data offset overflow"))?;
        }
    }

    // ═══════════════════════════════════════════════════════
    // v3: Dedup chunk + chunk table
    // ═══════════════════════════════════════════════════════
    use crate::chunk::DedupChunkMap;
    use crate::types::DonChunk;

    let mut chunk_map = DedupChunkMap::new();
    let mut chunk_table: Vec<DonChunk> = Vec::new();
    let mut chunk_data: Vec<u8> = Vec::new();

    // Chunk data start setelah semua Stored payload (bukan dari data_offset)
    let mut chunk_data_start_offset = data_offset;
    for item in prepared.iter() {
        if matches!(item.method, DonMethod::Stored | DonMethod::Rle | DonMethod::RleExt | DonMethod::Lz | DonMethod::Deflate) || item.chunks.is_empty() {
            chunk_data_start_offset += item.payload.len() as u64;
        }
    }

    for item in prepared.iter() {
        if item.chunks.is_empty() || matches!(item.method, DonMethod::Stored | DonMethod::Rle | DonMethod::RleExt | DonMethod::Lz | DonMethod::Deflate) {
            continue;  // skip Stored (payload langsung, bukan chunk)
        }
        for chunk in &item.chunks {
            let crc = chunk.crc32;
            if let Some(existing_offset) = chunk_map.lookup(crc) {
                chunk_table.push(DonChunk {
                    offset: existing_offset,
                    len: chunk.len,
                    crc32: chunk.crc32,
                });
            } else {
                let new_offset = chunk_data_start_offset + chunk_data.len() as u64;
                chunk_map.insert(crc, new_offset);
                let start_c = chunk.offset as usize;
                let end_c = start_c + chunk.len as usize;
                chunk_data.extend_from_slice(&item.payload[start_c..end_c]);
                chunk_table.push(DonChunk {
                    offset: new_offset,
                    len: chunk.len,
                    crc32: chunk.crc32,
                });
            }
        }
    }

    // Update entries: chunk_table_offset + chunk_count
    // Untuk method=Stored, chunk_count=0 (payload langsung, bukan chunk)
    let mut chunk_idx = 0u64;
    for (idx, item) in prepared.iter().enumerate() {
        if item.chunks.is_empty() || matches!(item.method, DonMethod::Stored | DonMethod::Rle | DonMethod::RleExt | DonMethod::Lz | DonMethod::Deflate) {
            entries[idx].chunk_table_offset = 0;
            entries[idx].chunk_count = 0;
            continue;
        }
        entries[idx].chunk_table_offset = chunk_idx * 16;
        entries[idx].chunk_count = item.chunks.len() as u32;
        chunk_idx += item.chunks.len() as u64;
    }

    // Serialize chunk table
    let mut chunk_table_bytes: Vec<u8> = Vec::with_capacity(chunk_table.len() * 16);
    for chunk in &chunk_table {
        chunk_table_bytes.extend_from_slice(&chunk.offset.to_le_bytes());
        chunk_table_bytes.extend_from_slice(&chunk.len.to_le_bytes());
        chunk_table_bytes.extend_from_slice(&chunk.crc32.to_le_bytes());
    }

    // REASSIGN data_offset + stored_size SEMUA entry (Stored + chunk)
    // URUTAN: Stored dulu (payload langsung), lalu chunk data (setelah Stored)
    let mut current_offset = data_offset;
    for (idx, item) in prepared.iter().enumerate() {
        if dedup_indices.contains(&idx) {
            if let Some(first_idx) = item.entry.dedup_source {
                entries[idx].data_offset = data_offsets[first_idx];
                entries[idx].stored_size = stored_sizes[first_idx];
            }
            continue;
        }
        if matches!(item.method, DonMethod::Stored | DonMethod::Rle | DonMethod::RleExt | DonMethod::Lz | DonMethod::Deflate) || item.chunks.is_empty() {
            entries[idx].data_offset = current_offset;
            entries[idx].stored_size = item.payload.len() as u64;
            current_offset += entries[idx].stored_size;
        }
    }
    // Setelah semua Stored, current_offset = chunk_data_start_offset (karena chunk data di akhir)
    // Tapi chunk_data_start_offset sudah dihitung dari data_offset + total Stored payload
    // Jadi chunk_offset_cursor mulai 0, data_offset chunk = chunk_data_start_offset + cursor
    let mut chunk_offset_cursor = 0u64;
    for (idx, item) in prepared.iter().enumerate() {
        if dedup_indices.contains(&idx) {
            continue;
        }
        if matches!(item.method, DonMethod::Stored | DonMethod::Rle | DonMethod::RleExt | DonMethod::Lz | DonMethod::Deflate) || item.chunks.is_empty() {
            continue;
        }
        // Chunk: data_offset = chunk_data_start_offset + cursor
        entries[idx].data_offset = chunk_data_start_offset + chunk_offset_cursor;
        entries[idx].stored_size = item.chunks.iter().map(|c| c.len as u64).sum();
        chunk_offset_cursor += entries[idx].stored_size;
    }

    // Baru build path table + encode entry (setelah chunk info di-set)
    let (raw_entries, path_table) = build_path_table(&entries)?;
    let entry_table_bytes = encode_raw_entries(&raw_entries);

    // Set header
    let chunk_table_offset = chunk_data_start_offset + chunk_data.len() as u64;
    let chunk_table_size = (chunk_table.len() * 16) as u64;

    let mut header = default_header();
    header.entry_count = entries.len() as u32;
    header.entry_table_offset = entry_table_offset;
    header.entry_table_size = entry_table_bytes.len() as u64;
    header.path_table_offset = path_table_offset;
    header.path_table_size = path_table.len() as u64;
    header.data_offset = data_offset;
    header.archive_crc32 = 0;
    header.chunk_table_offset = chunk_table_offset;
    header.chunk_table_size = chunk_table_size;
    
    // ═══════════════════════════════════════════════════════
    let parent = output_file.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let file = File::create(output_file)?;
    let mut writer = BufWriter::new(file);

    header.write_to(&mut writer)?;
    writer.write_all(&entry_table_bytes)?;
    writer.write_all(&path_table)?;

    // Write payload (skip entry yang dedup + chunk)
    for (idx, item) in prepared.iter().enumerate() {
        if dedup_indices.contains(&idx) {
            continue;
        }
        // Stored atau file kecil: payload langsung
        // Chunk: payload sudah di chunk_data
        if matches!(item.method, DonMethod::Stored | DonMethod::Rle | DonMethod::RleExt | DonMethod::Lz | DonMethod::Deflate) || item.chunks.is_empty() {
            writer.write_all(&item.payload)?;
        }
    }
    // Write chunk data (deduped)
    writer.write_all(&chunk_data)?;
    // Write chunk table
    writer.write_all(&chunk_table_bytes)?;

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
    #[allow(dead_code)]  // TODO: pakai di Step 2b-2f-4 (dedup chunk)
    chunks: Vec<crate::chunk::Chunk>,  // v3: chunk dedup
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
    use rayon::prelude::*;

    // Parallel per file: 8 core = 8 file kompres bersamaan
    let results: Vec<Result<PreparedEntry>> = items
        .par_iter()
        .map(|item| {
            let original = fs::read(&item.source_path)?;

            let (payload, method) = match pack_method {
                PackMethod::Store => (original.clone(), DonMethod::Stored),
                PackMethod::Rle => (don_rle::compress(&original), DonMethod::Rle),
                PackMethod::Lz => (don_lz::compress(&original), DonMethod::Lz),
                PackMethod::LzV5 => (don_lz::compress_lz_v5(&original), DonMethod::LzV5),
                PackMethod::LzV5Huff => (don_lz::compress_lz_v5_huff(&original), DonMethod::LzV5Huff),
                PackMethod::RleExt => (don_rle::compress_ext(&original), DonMethod::RleExt),
                PackMethod::Deflate => {
                    let level = std::env::var("DOLIN_DEFLATE_LEVEL")
                        .ok()
                        .and_then(|s| s.parse::<u8>().ok())
                        .unwrap_or(6);
                    eprintln!("[DEBUG] deflate level = {}", level);
                    (miniz_oxide::deflate::compress_to_vec(&original, level), DonMethod::Deflate)
                },
                PackMethod::Auto => {
                    // File sangat kecil (< 32 byte) → Store
                    if original.len() < 32 {
                        (original.clone(), DonMethod::Stored)
                    } else {
                        // Deteksi tipe file: magic number + ekstensi
                        let file_type = detect_file_type(&original, &item.rel_path);
                        match file_type {
                            "precompressed" => {
                                let is_zip_container = original.starts_with(b"PK\x03\x04");
                                
                                // File kecil (<4KB): overhead deflate tidak worth it
                                if original.len() < 4096 {
                                    (original.clone(), DonMethod::Stored)
                                } else if is_zip_container {
                                    // ZIP container: langsung deflate L1 (377ms for 25MB, worth it)
                                    let deflated = miniz_oxide::deflate::compress_to_vec(&original, 1);
                                    if deflated.len() < original.len() {
                                        (deflated, DonMethod::Deflate)
                                    } else {
                                        (original.clone(), DonMethod::Stored)
                                    }
                                } else {
                                    // Single-stream (MP4/JPEG): store
                                    (original.clone(), DonMethod::Stored)
                                }
                            },
                            "binary_img" => {
                                let rle_ext = don_rle::compress_ext(&original);
                                if rle_ext.len() < original.len() * 70 / 100 {
                                    (rle_ext, DonMethod::RleExt)
                                } else if original.len() < 4096 {
                                    (original.clone(), DonMethod::Stored)
                                } else if original.len() > 4096
                                    && estimate_entropy(&original[..original.len().min(64 * 1024)]) > 7.5
                                {
                                    (original.clone(), DonMethod::Stored)
                                } else {
                                    let lzv5 = don_lz::compress_lz_v5_typed(&original, "img");
                                    if lzv5.is_empty() || lzv5.len() >= original.len() {
                                        (original.clone(), DonMethod::Stored)
                                    } else {
                                        (lzv5, DonMethod::LzV5)
                                    }
                                }
                            }
                            "elf" => (don_lz::compress_lz_v5_typed(&original, "elf"), DonMethod::LzV5),
                            "text" => {
                                if original.len() < 4096 {
                                    let rle = don_rle::compress(&original);
                                    if rle.len() < original.len() {
                                        (rle, DonMethod::Rle)
                                    } else {
                                        (original.clone(), DonMethod::Stored)
                                    }
                                } else {
                                    (don_lz::compress_lz_v5_typed(&original, "text"), DonMethod::LzV5)
                                }
                            }
                            _ => {
                                // Fast entropy check: skip compression on random data
                                let sample_size = original.len().min(64 * 1024);
                                let sample = &original[..sample_size];
                                let entropy = estimate_entropy(sample);

                                if entropy > 7.5 {
                                    // High entropy = incompressible (random, encrypted)
                                    (original.clone(), DonMethod::Stored)
                                } else if original.len() < 4096 {
                                    // Small file: RLE or store (skip deflate header overhead)
                                    let rle = don_rle::compress(&original);
                                    if rle.len() < original.len() {
                                        (rle, DonMethod::Rle)
                                    } else {
                                        (original.clone(), DonMethod::Stored)
                                    }
                                } else {
                                    // Worth trying: deflate L1 (fast + good)
                                    let deflated = miniz_oxide::deflate::compress_to_vec(&original, 1);
                                    if deflated.len() < original.len() {
                                        (deflated, DonMethod::Deflate)
                                    } else {
                                        (original.clone(), DonMethod::Stored)
                                    }
                                }
                            }
                        }
                    }
                }
            };

            let mut entry = new_file_entry(
                item.rel_path.clone(),
                0,
                payload.len() as u64,
                original.len() as u64,
                item.crc32,
                method,
                item.mtime_unix,
            );
            entry.dedup_hash = item.crc32 as u64;

            // v3: chunk hash per file (4KB)
            let chunks = if payload.len() >= 4096
                && matches!(method, DonMethod::Lz | DonMethod::LzV5 | DonMethod::LzV5Huff)
            {
                crate::chunk::chunk_and_hash(&payload, 4096)
            } else {
                Vec::new()
            };

            Ok(PreparedEntry {
                entry,
                payload,
                original_size: original.len() as u64,
                crc32: item.crc32,
                method,
                mtime_unix: item.mtime_unix,
                chunks,
            })
        })
        .collect();

    // Propagate errors
    results.into_iter().collect()
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
