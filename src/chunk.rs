//! Dedup per chunk: pecah file jadi 4KB chunk, hash, dedup.

use std::collections::HashMap;

/// Chunk 4KB per file.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub offset: u64,   // offset di data section (0 = dedup, menunjuk chunk pertama)
    pub len: u32,      // panjang chunk (max 4096)
    pub crc32: u32,    // hash chunk
}

/// Map dedup chunk: crc32 → offset pertama.
pub struct DedupChunkMap {
    map: HashMap<u32, u64>,
}

impl DedupChunkMap {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn lookup(&self, crc: u32) -> Option<u64> {
        self.map.get(&crc).copied()
    }

    pub fn insert(&mut self, crc: u32, offset: u64) {
        self.map.insert(crc, offset);
    }
}

/// Pecah data jadi chunk 4KB, hash tiap chunk.
pub fn chunk_and_hash(data: &[u8], chunk_size: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;

    while offset < data.len() {
        let end = (offset + chunk_size).min(data.len());
        let chunk_data = &data[offset..end];
        let crc = crc32fast::hash(chunk_data);
        chunks.push(Chunk {
            offset: offset as u64,  // offset di payload (bukan di data section)
            len: (end - offset) as u32,
            crc32: crc,
        });
        offset = end;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_empty() {
        let chunks = chunk_and_hash(&[], 4096);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_small() {
        let chunks = chunk_and_hash(b"hello world", 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len, 11);
    }

    #[test]
    fn test_chunk_exact() {
        let chunks = chunk_and_hash(&vec![0u8; 4096], 4096);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len, 4096);
    }

    #[test]
    fn test_chunk_multi() {
        let chunks = chunk_and_hash(&vec![0u8; 10000], 4096);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len, 4096);
        assert_eq!(chunks[1].len, 4096);
        assert_eq!(chunks[2].len, 1808);
    }

    #[test]
    fn test_chunk_identical() {
        let chunks = chunk_and_hash(&vec![0u8; 8192], 4096);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].crc32, chunks[1].crc32);
    }

    #[test]
    fn test_dedup_map() {
        let mut map = DedupChunkMap::new();
        assert!(map.lookup(12345).is_none());
        map.insert(12345, 100);
        assert_eq!(map.lookup(12345), Some(100));
    }
}
