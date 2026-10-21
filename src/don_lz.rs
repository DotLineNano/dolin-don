use crate::error::DolinError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMethod {
    Store = 0,
    Lz = 1,
}

#[derive(Debug, Clone)]
pub struct EncodedBlock {
    pub method: CompressionMethod,
    pub original_size: usize,
    pub stored_size: usize,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Match {
    len: usize,
    offset: usize,
}

const NONE: usize = usize::MAX;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 127 + MIN_MATCH;
const WINDOW_SIZE: usize = 1 << 16;
const HASH_BITS: usize = 16;
const HASH_SIZE: usize = 1 << HASH_BITS;
const MAX_CHAIN: usize = 512;
const LAZY_THRESHOLD: usize = 1;
const GOOD_MATCH_LEN: usize = 8;
const MIN_MATCH_ADVANTAGE: usize = 2;
const VERY_GOOD_MATCH_LEN: usize = 16;
const FAR_OFFSET_THRESHOLD: usize = 8192;
const FAR_SHORT_MATCH_MAX: usize = 4;

#[inline]
fn hash4(data: &[u8], pos: usize) -> usize {
    let v = u32::from_le_bytes([
        data[pos],
        data[pos + 1],
        data[pos + 2],
        data[pos + 3],
    ]);
    ((v.wrapping_mul(2654435761) >> (32 - HASH_BITS)) as usize) & (HASH_SIZE - 1)
}

#[inline]
fn insert_pos(data: &[u8], pos: usize, head: &mut [usize], prev: &mut [usize]) {
    if pos + 4 > data.len() {
        return;
    }
    let h = hash4(data, pos);
    prev[pos] = head[h];
    head[h] = pos;
}

fn find_longest_match(
    data: &[u8],
    pos: usize,
    head: &[usize],
    prev: &[usize],
) -> Match {
    if pos + MIN_MATCH > data.len() || pos + 4 > data.len() {
        return Match::default();
    }

    let h = hash4(data, pos);
    let mut candidate = head[h];
    let mut best = Match::default();
    let mut traversed = 0;
    let max_len = MAX_MATCH.min(data.len() - pos);

    while candidate != NONE && traversed < MAX_CHAIN {
        if pos <= candidate {
            break;
        }

        let offset = pos - candidate;
        if offset > WINDOW_SIZE {
            break;
        }

        if data[candidate] != data[pos]
            || data[candidate + 1] != data[pos + 1]
            || data[candidate + 2] != data[pos + 2]
        {
            candidate = prev[candidate];
            traversed += 1;
            continue;
        }

        if best.len >= 4 {
            let probe = best.len.min(max_len - 1);
            if data[candidate + probe] != data[pos + probe] {
                candidate = prev[candidate];
                traversed += 1;
                continue;
            }
        }

        let mut len = 3;
        while len < max_len && data[candidate + len] == data[pos + len] {
            len += 1;
        }

        if offset >= FAR_OFFSET_THRESHOLD && len <= FAR_SHORT_MATCH_MAX {
            candidate = prev[candidate];
            traversed += 1;
            continue;
        }

        if len >= MIN_MATCH && (
            len > best.len ||
            (len == best.len && offset < best.offset)
        ) {
            best = Match { len, offset };

            if len >= VERY_GOOD_MATCH_LEN {
                break;
            }
            if len == max_len {
                break;
            }
        }

        candidate = prev[candidate];
        traversed += 1;
    }

    best
}

fn flush_literals(out: &mut Vec<u8>, data: &[u8], lit_start: usize, lit_len: usize) {
    let mut remaining = lit_len;
    let mut cursor = lit_start;

    while remaining > 0 {
        let chunk = remaining.min(127);
        out.push(chunk as u8);
        out.extend_from_slice(&data[cursor..cursor + chunk]);
        cursor += chunk;
        remaining -= chunk;
    }
}

fn emit_match(out: &mut Vec<u8>, m: Match) {
    let mut remaining = m.len;

    while remaining > 0 {
        let chunk_len = remaining.min(MAX_MATCH);
        let token_len = chunk_len - MIN_MATCH;
        out.push(0x80 | (token_len as u8));
        out.push((m.offset & 0xFF) as u8);
        out.push(((m.offset >> 8) & 0xFF) as u8);
        remaining -= chunk_len;
    }
}

#[inline]
fn should_delay_for_better_match(m1: Match, m2: Match) -> bool {
    m2.len >= MIN_MATCH
        && (
            m2.len > m1.len + LAZY_THRESHOLD
                || (m1.len <= GOOD_MATCH_LEN && m2.len >= m1.len + MIN_MATCH_ADVANTAGE)
                || (m1.len <= 4 && m2.len >= 6)
        )
}

#[inline]
fn match_worth_it(m: Match) -> bool {
    m.len >= 5 || (m.len == 4 && m.offset <= 4096) || (m.len == 3 && m.offset <= 256)
}

pub fn compress_lz_smart(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut head = vec![NONE; HASH_SIZE];
    let mut prev = vec![NONE; data.len()];
    let mut out = Vec::with_capacity(data.len() / 2);

    let mut pos = 0;
    let mut lit_start = 0;
    let mut lit_len = 0;

    while pos < data.len() {
        let can_match = pos + MIN_MATCH <= data.len() && pos + 4 <= data.len();

        let m1 = if can_match {
            find_longest_match(data, pos, &head, &prev)
        } else {
            Match::default()
        };

        if m1.len >= MIN_MATCH {
            let m2 = if pos + 1 + MIN_MATCH <= data.len() && pos + 1 + 4 <= data.len() {
                find_longest_match(data, pos + 1, &head, &prev)
            } else {
                Match::default()
            };

            if should_delay_for_better_match(m1, m2) {
                if lit_len == 0 {
                    lit_start = pos;
                }
                lit_len += 1;
                insert_pos(data, pos, &mut head, &mut prev);
                pos += 1;
            } else if match_worth_it(m1) {
                if lit_len > 0 {
                    flush_literals(&mut out, data, lit_start, lit_len);
                    lit_len = 0;
                }

                emit_match(&mut out, m1);

                for k in 0..m1.len {
                    if pos + k + 4 <= data.len() {
                        insert_pos(data, pos + k, &mut head, &mut prev);
                    }
                }

                pos += m1.len;
            } else {
                if lit_len == 0 {
                    lit_start = pos;
                }
                lit_len += 1;

                insert_pos(data, pos, &mut head, &mut prev);
                pos += 1;
            }
        } else {
            if lit_len == 0 {
                lit_start = pos;
            }
            lit_len += 1;

            if pos + 4 <= data.len() {
                insert_pos(data, pos, &mut head, &mut prev);
            }

            pos += 1;
        }
    }

    if lit_len > 0 {
        flush_literals(&mut out, data, lit_start, lit_len);
    }

    out
}

pub fn decompress_lz_smart(data: &[u8]) -> Result<Vec<u8>, DolinError> {
    let mut out = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let token = data[pos];
pos += 1;

        if (token & 0x80) == 0 {
            let len = token as usize;
            if pos + len > data.len() {
                return Err(DolinError::Format("lz literal token truncated".into()));
            }
            out.extend_from_slice(&data[pos..pos + len]);
            pos += len;
        } else {
            let len = ((token & 0x7F) as usize) + MIN_MATCH;
            if pos + 2 > data.len() {
                return Err(DolinError::Format("lz match token truncated".into()));
            }
            let offset = (data[pos] as usize) | ((data[pos + 1] as usize) << 8);
            pos += 2;

            if offset == 0 || offset > out.len() {
                return Err(DolinError::Format("lz invalid match offset".into()));
            }

            let start = out.len() - offset;
            for i in 0..len {
                let b = out[start + i];
                out.push(b);
            }
        }
    }

    Ok(out)
}

pub fn compress_or_store(data: &[u8]) -> EncodedBlock {
    let compressed = compress_lz_smart(data);

    if compressed.len() < data.len() {
        EncodedBlock {
            method: CompressionMethod::Lz,
            original_size: data.len(),
            stored_size: compressed.len(),
            data: compressed,
        }
    } else {
        EncodedBlock {
            method: CompressionMethod::Store,
            original_size: data.len(),
            stored_size: data.len(),
            data: data.to_vec(),
        }
    }
}

pub fn decompress_by_method(method: CompressionMethod, data: &[u8]) -> Result<Vec<u8>, DolinError> {
    match method {
        CompressionMethod::Store => Ok(data.to_vec()),
        CompressionMethod::Lz => decompress_lz_smart(data),
    }
}

pub fn compress(data: &[u8]) -> Vec<u8> {
    compress_lz_smart(data)
}

pub fn decompress(data: &[u8], expected_size: u64) -> Result<Vec<u8>, DolinError> {
    let out = decompress_lz_smart(data)?;
    if out.len() as u64 != expected_size {
        return Err(DolinError::Format(format!(
            "lz decompressed size mismatch: expected {}, got {}",
            expected_size,
            out.len()
        )));
    }
    Ok(out)
}

pub fn compress_don_lz(data: &[u8]) -> Vec<u8> {
    compress_lz_smart(data)
}

pub fn decompress_don_lz(data: &[u8], expected_size: u64) -> Result<Vec<u8>, DolinError> {
    decompress(data, expected_size)
}


#[cfg(test)]
mod ratio_info_tests {
    use super::*;

    #[test]
    fn test_ratio_info() {
        let samples: Vec<(&str, Vec<u8>)> = vec![
            ("zeroes_4k", vec![0u8; 4096]),
            ("repeated_text", b"hello hello hello hello hello hello hello hello".to_vec()),
            ("abc_repeat", b"abcabcabcabcabcabcabcabcabcabcabcabcabcabc".to_vec()),
            ("pattern_8k", (0..8192).map(|i| (i % 251) as u8).collect()),
        ];

        for (name, data) in samples {
            let c = compress_lz_smart(&data);
            eprintln!(
                "[ratio] {}: orig={} comp={} saved={}",
                name,
                data.len(),
                c.len(),
                data.len() as isize - c.len() as isize
            );
            let d = decompress_lz_smart(&c).unwrap();
            assert_eq!(d, data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(data: &[u8]) {
        let c = compress_lz_smart(data);
        let d = decompress_lz_smart(&c).unwrap();
        assert_eq!(d, data);
    }

    #[test]
    fn test_empty() {
        roundtrip(b"");
    }

    #[test]
    fn test_small() {
        roundtrip(b"a");
        roundtrip(b"abc");
        roundtrip(b"abcd");
    }

    #[test]
    fn test_repeated() {
        roundtrip(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        roundtrip(b"abcabcabcabcabcabcabcabcabcabcabc");
    }

    #[test]
    fn test_mixed() {
        let data = b"hello hello hello world world world hello hello";
        roundtrip(data);
    }

    #[test]
    fn test_zeroes() {
        let data = vec![0u8; 4096];
        roundtrip(&data);
    }

    #[test]
    fn test_binary_pattern() {
        let mut data = Vec::new();
        for i in 0..8192 {
            data.push((i % 251) as u8);
        }
        roundtrip(&data);
    }

    #[test]
    fn test_compress_or_store() {
        let data = vec![0u8; 1024];
        let enc = compress_or_store(&data);

        assert_eq!(enc.original_size, data.len());
        assert_eq!(enc.stored_size, enc.data.len());

        match enc.method {
            CompressionMethod::Store => {
                assert_eq!(enc.data, data);
            }
            CompressionMethod::Lz => {
                let out = decompress(enc.data.as_slice(), data.len() as u64).unwrap();
                assert_eq!(out, data);
            }
        }
    }
}




fn write_len_ext(out: &mut Vec<u8>, mut extra: usize) {
    while extra >= 255 {
        out.push(255);
        extra -= 255;
    }
    out.push(extra as u8);
}

fn read_len_ext(data: &[u8], pos: &mut usize, mut len: usize) -> Result<usize, DolinError> {
    loop {
        if *pos >= data.len() {
            return Err(DolinError::Format("lz v5 length extension truncated".into()));
        }
        let b = data[*pos] as usize;
        *pos += 1;
        len += b;
        if b != 255 {
            break;
        }
    }
    Ok(len)
}

pub fn compress_lz_v5(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(data.len() / 2);
    let mut head = vec![usize::MAX; HASH_SIZE];
    let mut prev = vec![usize::MAX; data.len()];
    let mut anchor = 0usize;
    let mut pos = 0usize;

    while pos + MIN_MATCH <= data.len() {
        let m = find_longest_match(data, pos, &head, &prev);

        if !match_worth_it(m) {
            if pos + 4 <= data.len() {
                insert_pos(data, pos, &mut head, &mut prev);
            }
            pos += 1;
            continue;
        }

        let lit_len = pos - anchor;
        let lit_nib = lit_len.min(15);
        let match_extra = m.len - MIN_MATCH;
        let match_nib = match_extra.min(15);

        let token = ((lit_nib as u8) << 4) | (match_nib as u8);
        out.push(token);

        if lit_len >= 15 {
            write_len_ext(&mut out, lit_len - 15);
        }

        out.extend_from_slice(&data[anchor..pos]);

        let offset = m.offset as u16;
        out.push((offset & 0xFF) as u8);
        out.push((offset >> 8) as u8);

        if match_extra >= 15 {
            write_len_ext(&mut out, match_extra - 15);
        }

        for k in 0..m.len {
            if pos + k + 4 <= data.len() {
                insert_pos(data, pos + k, &mut head, &mut prev);
            }
        }

        pos += m.len;
        anchor = pos;
    }

    let lit_len = data.len() - anchor;
    if lit_len > 0 {
        let lit_nib = lit_len.min(15);
        let token = (lit_nib as u8) << 4;
        out.push(token);

        if lit_len >= 15 {
            write_len_ext(&mut out, lit_len - 15);
        }

        out.extend_from_slice(&data[anchor..]);
    }

    out
}

pub fn decompress_lz_v5(data: &[u8], expected_size: u64) -> Result<Vec<u8>, DolinError> {
    let mut out = Vec::with_capacity(expected_size as usize);
    let mut pos = 0usize;

    while pos < data.len() {
        let token = data[pos];
        pos += 1;

        let lit_base = ((token >> 4) & 0x0F) as usize;
let mut lit_len = lit_base;
        if lit_base == 15 {
            lit_len = read_len_ext(data, &mut pos, lit_len)?;
        }
if pos + lit_len > data.len() {
            return Err(DolinError::Format("lz v5 literal truncated".into()));
        }

        out.extend_from_slice(&data[pos..pos + lit_len]);
        pos += lit_len;
if pos >= data.len() {
break;
        }

        if pos + 2 > data.len() {
            return Err(DolinError::Format("lz v5 offset truncated".into()));
        }

        let offset = (data[pos] as usize) | ((data[pos + 1] as usize) << 8);
        pos += 2;
if offset == 0 || offset > out.len() {
            return Err(DolinError::Format("lz v5 invalid match offset".into()));
        }

        let match_base = (token & 0x0F) as usize;
        let mut match_len = match_base + MIN_MATCH;
        if match_base == 15 {
match_len = read_len_ext(data, &mut pos, match_len)?;
        }
let start = out.len() - offset;
for i in 0..match_len {
            let b = out[start + i];
            out.push(b);
        }
}

    if out.len() as u64 != expected_size {
        return Err(DolinError::Format("lz v5 decoded size mismatch".into()));
    }

    Ok(out)
}

#[cfg(test)]
mod lz_v5_tests {
    use super::*;

    #[test]
    fn test_lz_v5_empty() {
        let data = b"";
        let enc = compress_lz_v5(data);
        let dec = decompress_lz_v5(&enc, data.len() as u64).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn test_lz_v5_repeated() {
        let data = b"hello hello hello hello hello";
        let enc = compress_lz_v5(data);
        let dec = decompress_lz_v5(&enc, data.len() as u64).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn test_lz_v5_mixed() {
        let data = b"abcabcabcXYZXYZXYZabcabcabc123123123---abcabc";
        let enc = compress_lz_v5(data);
        let dec = decompress_lz_v5(&enc, data.len() as u64).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn test_lz_v5_binary_pattern() {
        let mut data = Vec::new();
        for i in 0..1024usize {
            data.push((i % 251) as u8);
        }
        data.extend_from_within(100..700);
        data.extend_from_within(50..500);

        let enc = compress_lz_v5(&data);
        let dec = decompress_lz_v5(&enc, data.len() as u64).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn test_lz_v5_zeroes() {
        let data = vec![0u8; 4096];
        let enc = compress_lz_v5(&data);
        let dec = decompress_lz_v5(&enc, data.len() as u64).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn test_lz_v5_trace_small_repeated() {
        let data = b"abcabcabcabc";
        let enc = compress_lz_v5(data);
        let dec = decompress_lz_v5(&enc, data.len() as u64).unwrap();
        assert_eq!(dec, data);
    }

    #[test]
    fn test_lz_v5_expected_size_mismatch_rejected() {
        let data = b"some repeated repeated repeated bytes";
        let enc = compress_lz_v5(data);
        let err = decompress_lz_v5(&enc, (data.len() as u64) + 1).unwrap_err();
        let msg = format!("{err:?}").to_lowercase();
        assert!(msg.contains("size"));
    }
}
