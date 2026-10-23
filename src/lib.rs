pub mod api;
pub mod chunk;
pub mod don_lz;
pub mod huffman;
pub mod don_rle;
pub mod error;
pub mod format;
pub mod types;
pub mod util;

mod info;
pub use api::{DonArchiveInfo, DonEntryInfo};
pub use error::{DolinError, Result};
pub use format::reader::{
    extract_archive_entry, extract_archive_to_dir, extract_archive_to_dir_with,
    ExtractOptions, read_archive_entries as list_archive_entries,
};
pub use format::validate::validate_archive;
pub use format::writer::{pack_path_to_archive, pack_path_to_archive_with_method, PackMethod};

pub fn bench_rle_size(data: &[u8]) -> usize {
    don_rle::compress(data).len()
}

pub fn bench_lz_size(data: &[u8]) -> usize {
    don_lz::compress(data).len()
}

pub fn bench_lz_v5_size(data: &[u8]) -> usize {
    don_lz::compress_lz_v5(data).len()
}

pub fn get_archive_info(archive_path: &std::path::Path) -> Result<DonArchiveInfo> {
    let entries = read_archive_entries_internal(archive_path)?;

    let entry_count = entries.len() as u32;
    let mut seen_payloads: std::collections::HashSet<(u32, u64)> = std::collections::HashSet::new();
        let total_stored_size: u64 = entries.iter()
            .filter(|e| seen_payloads.insert((e.crc32, e.original_size)))
            .map(|e| e.stored_size)
            .sum();
    let total_original_size = entries.iter().map(|e| e.original_size).sum();

    Ok(DonArchiveInfo {
        entry_count,
        total_stored_size,
        total_original_size,
    })
}

fn read_archive_entries_internal(archive_path: &std::path::Path) -> Result<Vec<DonEntryInfo>> {
    format::reader::read_archive_entries(archive_path)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::format::reader::{
        extract_archive_entry, extract_archive_to_dir, read_archive_entries,
    };
    use crate::format::validate::validate_archive;
    use crate::format::writer::pack_path_to_archive;

    #[test]
    fn roundtrip_pack_validate_list_extract() {
        let base = make_temp_dir("dolin_roundtrip");
        let input_dir = base.join("input");
        let output_archive = base.join("sample.don");
        let extract_dir = base.join("output");

        fs::create_dir_all(input_dir.join("sub")).unwrap();
        fs::write(input_dir.join("a.txt"), b"hello\n").unwrap();
        fs::write(input_dir.join("sub").join("b.txt"), b"world\n").unwrap();

        pack_path_to_archive(&input_dir, &output_archive).unwrap();

        validate_archive(&output_archive).unwrap();

        let mut entries = read_archive_entries(&output_archive).unwrap();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].path, "a.txt");
        assert_eq!(entries[0].stored_size, 6);
        assert_eq!(entries[0].original_size, 6);
        assert_eq!(entries[0].method, 0);

        assert_eq!(entries[1].path, "sub/b.txt");
        assert_eq!(entries[1].stored_size, 6);
        assert_eq!(entries[1].original_size, 6);
        assert_eq!(entries[1].method, 0);

        extract_archive_to_dir(&output_archive, &extract_dir).unwrap();

        let a = fs::read(extract_dir.join("a.txt")).unwrap();
        let b = fs::read(extract_dir.join("sub").join("b.txt")).unwrap();

        assert_eq!(a, b"hello\n");
        assert_eq!(b, b"world\n");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn validate_fails_when_payload_is_corrupted() {
        let base = make_temp_dir("dolin_crc_fail");
        let input_dir = base.join("input");
        let archive_path = base.join("sample.don");

        fs::create_dir_all(input_dir.join("sub")).unwrap();
        fs::write(input_dir.join("a.txt"), b"hello\n").unwrap();
        fs::write(input_dir.join("sub").join("b.txt"), b"world\n").unwrap();

        pack_path_to_archive(&input_dir, &archive_path).unwrap();
        validate_archive(&archive_path).unwrap();

        let mut bytes = fs::read(&archive_path).unwrap();
        assert!(!bytes.is_empty());

        // Corrupt byte di payload (bukan header/path table).
        // Ambil data_offset dari header (offset 48..56 di header v2).
        use crate::types::DON_HEADER_SIZE;
        let data_offset = u64::from_le_bytes([
            bytes[48], bytes[49], bytes[50], bytes[51],
            bytes[52], bytes[53], bytes[54], bytes[55],
        ]) as usize;
        let idx = if bytes.len() > data_offset + 4 {
            data_offset + 4
        } else {
            bytes.len() - 1
        };
        bytes[idx] ^= 0xFF;
        fs::write(&archive_path, &bytes).unwrap();

        let err = validate_archive(&archive_path).unwrap_err();
        let msg = err.to_string();

        assert!(msg.contains("crc32 mismatch"), "unexpected error: {}", msg);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn extract_single_entry_only() {
        let base = make_temp_dir("dolin_extract_one");
        let input_dir = base.join("input");
        let archive_path = base.join("sample.don");
        let output_file = base.join("picked.txt");

        fs::create_dir_all(input_dir.join("sub")).unwrap();
        fs::write(input_dir.join("a.txt"), b"hello\n").unwrap();
        fs::write(input_dir.join("sub").join("b.txt"), b"world\n").unwrap();

        pack_path_to_archive(&input_dir, &archive_path).unwrap();
        validate_archive(&archive_path).unwrap();

        extract_archive_entry(&archive_path, "sub/b.txt", &output_file).unwrap();

        let content = fs::read(&output_file).unwrap();
        assert_eq!(content, b"world\n");

        assert!(!output_file.with_file_name(".picked.txt.part").exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pack_single_file_input() {
        let base = make_temp_dir("dolin_single_file");
        let input_file = base.join("lonely.txt");
        let archive_path = base.join("single.don");
        let extract_dir = base.join("output");

        fs::write(&input_file, b"just-one-file\n").unwrap();

        pack_path_to_archive(&input_file, &archive_path).unwrap();
        validate_archive(&archive_path).unwrap();

        let entries = read_archive_entries(&archive_path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "lonely.txt");
        assert_eq!(entries[0].stored_size, 14);
        assert_eq!(entries[0].original_size, 14);
        assert_eq!(entries[0].method, 0);

        extract_archive_to_dir(&archive_path, &extract_dir).unwrap();

        let content = fs::read(extract_dir.join("lonely.txt")).unwrap();
        assert_eq!(content, b"just-one-file\n");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pack_rejects_output_inside_input_dir() {
        let base = make_temp_dir("dolin_output_guard");
        let input_dir = base.join("input");
        let nested_output = input_dir.join("nested").join("bad.don");

        fs::create_dir_all(input_dir.join("nested")).unwrap();
        fs::write(input_dir.join("a.txt"), b"hello\n").unwrap();

        let err = pack_path_to_archive(&input_dir, &nested_output).unwrap_err();
        let msg = err.to_string();

        assert!(msg.contains("invalid path"), "unexpected error: {}", msg);

        assert!(!nested_output.exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pack_extract_random_incompressible_data() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_random_incompressible_data_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("random.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        let data: Vec<u8> = (0..65536).map(|i| ((i * 73 + 19) % 251) as u8).collect();

        std::fs::write(input_dir.join("random.bin"), &data).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        let restored_data = std::fs::read(restored.join("random.bin")).unwrap();
        assert_eq!(restored_data, data);

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "random.bin");
        assert_eq!(entries[0].original_size as usize, data.len());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_large_file_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("dolin_pack_extract_large_file_roundtrip_{}", uniq));
        let input_dir = root.join("input");
        let output = root.join("large.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        let mut data = Vec::with_capacity(2 * 1024 * 1024);
        for i in 0..(2 * 1024 * 1024) {
            data.push(((i * 17 + 31) % 256) as u8);
        }

        std::fs::write(input_dir.join("large.bin"), &data).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        let restored_data = std::fs::read(restored.join("large.bin")).unwrap();
        assert_eq!(restored_data, data);

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "large.bin");
        assert_eq!(entries[0].original_size as usize, data.len());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_unicode_filename_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_unicode_filename_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("unicode.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        let name = "héllo_日本_🔥.txt";
        let data = b"unicode payload contents\nline2\n".to_vec();

        std::fs::write(input_dir.join(name), &data).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        let restored_data = std::fs::read(restored.join(name)).unwrap();
        assert_eq!(restored_data, data);

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, name);
        assert_eq!(entries[0].original_size as usize, data.len());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_zero_byte_file_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_zero_byte_file_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("zero.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        std::fs::write(input_dir.join("empty.bin"), b"").unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        let restored_data = std::fs::read(restored.join("empty.bin")).unwrap();
        assert!(restored_data.is_empty());

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "empty.bin");
        assert_eq!(entries[0].original_size, 0);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_nested_mixed_tree_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_nested_mixed_tree_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("tree.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(input_dir.join("a/b")).unwrap();
        std::fs::create_dir_all(input_dir.join("c")).unwrap();

        let alpha = b"alpha alpha alpha alpha alpha\n".to_vec();
        let beta: Vec<u8> = (0..4096).map(|i| ((i * 29 + 7) % 251) as u8).collect();
        let gamma = b"".to_vec();

        std::fs::write(input_dir.join("root.txt"), &alpha).unwrap();
        std::fs::write(input_dir.join("a/b/random.bin"), &beta).unwrap();
        std::fs::write(input_dir.join("c/empty.txt"), &gamma).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        assert_eq!(std::fs::read(restored.join("root.txt")).unwrap(), alpha);
        assert_eq!(
            std::fs::read(restored.join("a/b/random.bin")).unwrap(),
            beta
        );
        assert_eq!(std::fs::read(restored.join("c/empty.txt")).unwrap(), gamma);

        let mut paths: Vec<String> = crate::list_archive_entries(&output)
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                "a/b/random.bin".to_string(),
                "c/empty.txt".to_string(),
                "root.txt".to_string(),
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_many_small_files_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_many_small_files_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("many_small.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        for i in 0..128usize {
            let name = format!("file_{:03}.txt", i);
            let data = format!("payload-{}-{}\n", i, i * 17);
            std::fs::write(input_dir.join(&name), data.as_bytes()).unwrap();
        }

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        for i in 0..128usize {
            let name = format!("file_{:03}.txt", i);
            let data = format!("payload-{}-{}\n", i, i * 17).into_bytes();
            assert_eq!(std::fs::read(restored.join(&name)).unwrap(), data);
        }

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 128);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_long_nested_path_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_long_nested_path_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("long_path.don");
        let restored = root.join("restored");

        let rel_dir =
            "segment01/segment02/segment03/segment04/segment05/segment06/segment07/segment08";
        let rel_file = format!("{}/deep_file.txt", rel_dir);
        let data = b"deep path payload\n".to_vec();

        std::fs::create_dir_all(input_dir.join(rel_dir)).unwrap();
        std::fs::write(input_dir.join(&rel_file), &data).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        assert_eq!(std::fs::read(restored.join(&rel_file)).unwrap(), data);

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, rel_file);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_filenames_with_spaces_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_filenames_with_spaces_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("spaces.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(input_dir.join("dir with spaces")).unwrap();

        let file1 = "hello world.txt";
        let file2 = "dir with spaces/nested file name.bin";

        let data1 = b"hello spaced world\n".to_vec();
        let data2: Vec<u8> = (0..1024).map(|i| ((i * 11 + 5) % 251) as u8).collect();

        std::fs::write(input_dir.join(file1), &data1).unwrap();
        std::fs::write(input_dir.join(file2), &data2).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        assert_eq!(std::fs::read(restored.join(file1)).unwrap(), data1);
        assert_eq!(std::fs::read(restored.join(file2)).unwrap(), data2);

        let mut paths: Vec<String> = crate::list_archive_entries(&output)
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                "dir with spaces/nested file name.bin".to_string(),
                "hello world.txt".to_string(),
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_mixed_extensions_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_mixed_extensions_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("mixed_ext.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(input_dir.join("lib")).unwrap();

        let txt = b"plain text payload\n".to_vec();
        let json = br#"{"name":"dolin","kind":"archive-test"}"#.to_vec();
        let bin: Vec<u8> = (0..2048).map(|i| ((i * 13 + 9) % 251) as u8).collect();
        let so_like: Vec<u8> = (0..4096).map(|i| ((i * 7 + 3) % 256) as u8).collect();

        std::fs::write(input_dir.join("note.txt"), &txt).unwrap();
        std::fs::write(input_dir.join("meta.json"), &json).unwrap();
        std::fs::write(input_dir.join("data.bin"), &bin).unwrap();
        std::fs::write(input_dir.join("lib/libsample.so"), &so_like).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        assert_eq!(std::fs::read(restored.join("note.txt")).unwrap(), txt);
        assert_eq!(std::fs::read(restored.join("meta.json")).unwrap(), json);
        assert_eq!(std::fs::read(restored.join("data.bin")).unwrap(), bin);
        assert_eq!(
            std::fs::read(restored.join("lib/libsample.so")).unwrap(),
            so_like
        );

        let mut paths: Vec<String> = crate::list_archive_entries(&output)
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                "data.bin".to_string(),
                "lib/libsample.so".to_string(),
                "meta.json".to_string(),
                "note.txt".to_string(),
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_512_small_files_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_512_small_files_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("many_512.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        for i in 0..512usize {
            let name = format!("tiny_{:03}.txt", i);
            let data = format!("tiny-payload-{}-{}\n", i, i * 31);
            std::fs::write(input_dir.join(&name), data.as_bytes()).unwrap();
        }

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        for i in 0..512usize {
            let name = format!("tiny_{:03}.txt", i);
            let data = format!("tiny-payload-{}-{}\n", i, i * 31).into_bytes();
            assert_eq!(std::fs::read(restored.join(&name)).unwrap(), data);
        }

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 512);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_long_filename_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_long_filename_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("long_name.don");
        let restored = root.join("restored");

        std::fs::create_dir_all(&input_dir).unwrap();

        let long_name = format!("{}_{}.txt", "a".repeat(120), "tail");
        let data = b"long filename payload\n".to_vec();

        std::fs::write(input_dir.join(&long_name), &data).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        assert_eq!(std::fs::read(restored.join(&long_name)).unwrap(), data);

        let entries = crate::list_archive_entries(&output).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, long_name);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_extract_combo_path_torture_roundtrip() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let uniq = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "dolin_pack_extract_combo_path_torture_roundtrip_{}",
            uniq
        ));
        let input_dir = root.join("input");
        let output = root.join("combo.don");
        let restored = root.join("restored");

        let dir1 = "日本 space/深い dir/level 03";
        let dir2 = "zero zone";
        let file1 = format!("{}/héllo world.bin", dir1);
        let file2 = format!("{}/empty file.txt", dir2);
        let file3 = "root スペース.json";

        std::fs::create_dir_all(input_dir.join(dir1)).unwrap();
        std::fs::create_dir_all(input_dir.join(dir2)).unwrap();

        let data1: Vec<u8> = (0..8192).map(|i| ((i * 19 + 23) % 251) as u8).collect();
        let data2: Vec<u8> = vec![];
        let data3 = br#"{"ok":true,"name":"combo"}"#.to_vec();

        std::fs::write(input_dir.join(&file1), &data1).unwrap();
        std::fs::write(input_dir.join(&file2), &data2).unwrap();
        std::fs::write(input_dir.join(file3), &data3).unwrap();

        pack_path_to_archive(&input_dir, &output).unwrap();
        validate_archive(&output).unwrap();
        extract_archive_to_dir(&output, &restored).unwrap();

        assert_eq!(std::fs::read(restored.join(&file1)).unwrap(), data1);
        assert_eq!(std::fs::read(restored.join(&file2)).unwrap(), data2);
        assert_eq!(std::fs::read(restored.join(file3)).unwrap(), data3);

        let mut paths: Vec<String> = crate::list_archive_entries(&output)
            .unwrap()
            .into_iter()
            .map(|e| e.path)
            .collect();
        paths.sort();

        assert_eq!(
            paths,
            vec![
                "root スペース.json".to_string(),
                "zero zone/empty file.txt".to_string(),
                "日本 space/深い dir/level 03/héllo world.bin".to_string(),
            ]
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn extract_all_rolls_back_on_crc_failure() {
        let base = make_temp_dir("dolin_extract_rollback");
        let input_dir = base.join("input");
        let archive_path = base.join("sample.don");
        let output_dir = base.join("broken_out");

        fs::create_dir_all(input_dir.join("sub")).unwrap();
        fs::write(input_dir.join("a.txt"), b"hello\n").unwrap();
        fs::write(input_dir.join("sub").join("b.txt"), b"world\n").unwrap();

        pack_path_to_archive(&input_dir, &archive_path).unwrap();

        let mut bytes = fs::read(&archive_path).unwrap();
        assert!(!bytes.is_empty());

        // Corrupt byte di payload (bukan header/path table).
        // Ambil data_offset dari header (offset 48..56 di header v2).
        use crate::types::DON_HEADER_SIZE;
        let data_offset = u64::from_le_bytes([
            bytes[48], bytes[49], bytes[50], bytes[51],
            bytes[52], bytes[53], bytes[54], bytes[55],
        ]) as usize;
        let idx = if bytes.len() > data_offset + 4 {
            data_offset + 4
        } else {
            bytes.len() - 1
        };
        bytes[idx] ^= 0xFF;
        fs::write(&archive_path, &bytes).unwrap();

        let err = extract_archive_to_dir(&archive_path, &output_dir).unwrap_err();
        let msg = err.to_string();

        assert!(msg.contains("crc32 mismatch"), "unexpected error: {}", msg);

        assert!(!output_dir.exists());
        assert!(!base.join(".broken_out.partial").exists());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pack_uses_rle_for_repetitive_file() {
        let base = make_temp_dir("dolin_rle_specific");
        let input_dir = base.join("input");
        let archive_path = base.join("sample_rle.don");
        let extract_dir = base.join("output");

        fs::create_dir_all(&input_dir).unwrap();
        fs::write(input_dir.join("repeat.txt"), vec![b'A'; 5000]).unwrap();

        pack_path_to_archive(&input_dir, &archive_path).unwrap();
        validate_archive(&archive_path).unwrap();

        let entries = read_archive_entries(&archive_path).unwrap();
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];
        assert_eq!(entry.path, "repeat.txt");
        assert_eq!(entry.original_size, 5000);
        // RLE atau LzV5 — keduanya valid untuk file repetitif.
        // Dengan MAX_MATCH_V5, LzV5 bisa lebih kecil dari RLE.
        assert!(
            entry.method == 1 || entry.method == 3,
            "expected compressed method (RLE or LzV5), got {}",
            entry.method
        );
        assert!(
            entry.stored_size < entry.original_size,
            "expected compressed payload to be smaller: stored={} original={}",
            entry.stored_size,
            entry.original_size
        );

        extract_archive_to_dir(&archive_path, &extract_dir).unwrap();

        let original = fs::read(input_dir.join("repeat.txt")).unwrap();
        let extracted = fs::read(extract_dir.join("repeat.txt")).unwrap();
        assert_eq!(original, extracted);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pack_uses_lz_for_repeated_phrase_file() {
        let base = make_temp_dir("dolin_lz_specific");
        let input_dir = base.join("input");
        let archive_path = base.join("sample_lz.don");
        let extract_dir = base.join("output");

        fs::create_dir_all(&input_dir).unwrap();

        let mut data = Vec::new();
        for _ in 0..400 {
            data.extend_from_slice(b"hello-rust-archive\n");
        }
        fs::write(input_dir.join("phrase.txt"), &data).unwrap();

        pack_path_to_archive(&input_dir, &archive_path).unwrap();
        validate_archive(&archive_path).unwrap();

        let entries = read_archive_entries(&archive_path).unwrap();
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];
        assert_eq!(entry.path, "phrase.txt");
        assert_eq!(entry.original_size, data.len() as u64);
        // Harus terkompresi (bukan Stored). Kompresor mana yang menang
        // tidak penting — RLE, LZ, atau LZ V5 semuanya valid untuk
        // file frasa berulang.
        assert_ne!(
            entry.method, 0,
            "expected compressed (non-Stored) method for repeated phrase file, got {}",
            entry.method
        );
        assert!(
            entry.stored_size < entry.original_size,
            "expected compressed payload to be smaller: stored={} original={}",
            entry.stored_size,
            entry.original_size
        );

        extract_archive_to_dir(&archive_path, &extract_dir).unwrap();

        let original = fs::read(input_dir.join("phrase.txt")).unwrap();
        let extracted = fs::read(extract_dir.join("phrase.txt")).unwrap();
        assert_eq!(original, extracted);

        let _ = fs::remove_dir_all(&base);
    }

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        dir.push(format!("{}_{}_{}", prefix, std::process::id(), now));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}

pub use crate::info::{archive_info, ArchiveInfo};
pub mod report;
pub use crate::api::{
    archive_info_report, extract_archive_entry_report, extract_archive_report, list_archive_report,
    pack_path_report, validate_archive_report,
};
pub use crate::api::{
    archive_info_report_json, extract_archive_entry_report_json, extract_archive_report_json,
    list_archive_report_json, pack_path_report_json, validate_archive_report_json,
};
pub use crate::report::{
    ArchiveEntryReport, ArchiveInfoReport, ExtractOneReport, ExtractReport, PackReport,
    ValidateReport,
};
pub mod jni;
pub use don_lz::{
    compress_lz_smart, compress_lz_v5, decompress as decompress_lz, decompress_lz_v5,
};
pub use don_rle::{compress as compress_don_rle, decompress as decompress_don_rle};
