use crate::{DolinError, Result};

const MIN_RUN: usize = 3;
const MAX_LITERAL: usize = 128;
const MAX_RUN: usize = 130;

pub fn compress(input: &[u8]) -> Vec<u8> {
    if input.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(input.len() / 2);
    let mut i = 0usize;
    let mut lit_start = 0usize;
    let mut lit_len = 0usize;

    while i < input.len() {
        let run_len = count_run_capped(input, i, MAX_RUN);

        if run_len >= MIN_RUN {
            if lit_len > 0 {
                flush_literals(&mut out, input, lit_start, lit_len);
                lit_len = 0;
            }

            let tag = 0x80 | ((run_len - MIN_RUN) as u8);
            out.push(tag);
            out.push(input[i]);
            i += run_len;
            continue;
        }

        if lit_len == 0 {
            lit_start = i;
        }

        lit_len += 1;
        i += 1;

        if lit_len == MAX_LITERAL {
            flush_literals(&mut out, input, lit_start, lit_len);
            lit_len = 0;
        }
    }

    if lit_len > 0 {
        flush_literals(&mut out, input, lit_start, lit_len);
    }

    out
}

pub fn decompress(input: &[u8], expected_size: u64) -> Result<Vec<u8>> {
    let expected_size = usize::try_from(expected_size)
        .map_err(|_| DolinError::Format("expected size does not fit usize".into()))?;

    if expected_size == 0 {
        if input.is_empty() {
            return Ok(Vec::new());
        } else {
            return Err(DolinError::Format(
                "non-empty DON-RLE stream for empty output".into(),
            ));
        }
    }

    let mut out = Vec::with_capacity(expected_size);
    let mut i = 0usize;

    while i < input.len() && out.len() < expected_size {
        let tag = input[i];
        i += 1;

        if tag < 0x80 {
            let len = ((tag & 0x7f) as usize) + 1;
            let end = i
                .checked_add(len)
                .ok_or_else(|| DolinError::Format("literal packet overflow".into()))?;

            if end > input.len() {
                return Err(DolinError::Format(
                    "truncated DON-RLE literal packet".into(),
                ));
            }

            let new_len = out
                .len()
                .checked_add(len)
                .ok_or_else(|| DolinError::Format("decompressed size overflow".into()))?;

            if new_len > expected_size {
                return Err(DolinError::Format(
                    "DON-RLE output exceeds expected size".into(),
                ));
            }

            out.extend_from_slice(&input[i..end]);
            i = end;
        } else {
            let len = ((tag & 0x7f) as usize) + MIN_RUN;

            if i >= input.len() {
                return Err(DolinError::Format("truncated DON-RLE run packet".into()));
            }

            let value = input[i];
            i += 1;

            let new_len = out
                .len()
                .checked_add(len)
                .ok_or_else(|| DolinError::Format("decompressed size overflow".into()))?;

            if new_len > expected_size {
                return Err(DolinError::Format(
                    "DON-RLE output exceeds expected size".into(),
                ));
            }

            out.resize(new_len, value);
        }
    }

    if out.len() != expected_size {
        return Err(DolinError::Format(
            "DON-RLE output shorter than expected size".into(),
        ));
    }

    if i != input.len() {
        return Err(DolinError::Format(
            "trailing bytes after DON-RLE stream".into(),
        ));
    }

    Ok(out)
}

fn flush_literals(out: &mut Vec<u8>, input: &[u8], start: usize, len: usize) {
    debug_assert!(len >= 1 && len <= MAX_LITERAL);
    let tag = (len - 1) as u8;
    out.push(tag);
    out.extend_from_slice(&input[start..start + len]);
}

fn count_run_capped(input: &[u8], start: usize, cap: usize) -> usize {
    let byte = input[start];
    let mut len = 1usize;
    let end = (start + cap).min(input.len());

    while start + len < end && input[start + len] == byte {
        len += 1;
    }

    len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_empty() {
        let encoded = compress(&[]);
        assert!(encoded.is_empty());
    }

    #[test]
    fn roundtrip_literal_only() {
        let input = b"abcdef";
        let encoded = compress(input);
        let decoded = decompress(&encoded, input.len() as u64).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn roundtrip_single_run() {
        let input = b"aaaaaa";
        let encoded = compress(input);
        let decoded = decompress(&encoded, input.len() as u64).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn roundtrip_mixed() {
        let input = b"\x00\x00\x00\x00\x11\x22\x33\x44\x44\x44";
        let encoded = compress(input);
        let decoded = decompress(&encoded, input.len() as u64).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn roundtrip_long_run_split() {
        let input = vec![b'A'; 300];
        let encoded = compress(&input);
        let decoded = decompress(&encoded, input.len() as u64).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn roundtrip_long_literal_split() {
        let input: Vec<u8> = (0..200).map(|i| (i % 251) as u8).collect();
        let encoded = compress(&input);
        let decoded = decompress(&encoded, input.len() as u64).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn decompress_rejects_truncated_literal() {
        let encoded = vec![0x02, b'a', b'b'];
        let err = decompress(&encoded, 3).unwrap_err();
        assert!(matches!(err, DolinError::Format(_)));
    }

    #[test]
    fn decompress_rejects_truncated_run() {
        let encoded = vec![0x80];
        let err = decompress(&encoded, 3).unwrap_err();
        assert!(matches!(err, DolinError::Format(_)));
    }

    #[test]
    fn decompress_rejects_short_output() {
        let encoded = vec![0x00, b'a'];
        let err = decompress(&encoded, 2).unwrap_err();
        assert!(matches!(err, DolinError::Format(_)));
    }

    #[test]
    fn decompress_rejects_trailing_bytes() {
        let encoded = vec![0x00, b'a', 0x00, b'b'];
        let err = decompress(&encoded, 1).unwrap_err();
        assert!(matches!(err, DolinError::Format(_)));
    }

    #[test]
    fn decompress_rejects_nonempty_stream_for_empty_output() {
        let encoded = vec![0x00, b'a'];
        let err = decompress(&encoded, 0).unwrap_err();
        assert!(matches!(err, DolinError::Format(_)));
    }

    #[test]
    fn compress_zeroes_fast_shape() {
        let input = vec![0u8; 1024];
        let encoded = compress(&input);
        let decoded = decompress(&encoded, input.len() as u64).unwrap();
        assert_eq!(decoded, input);
        assert!(!encoded.is_empty());
    }
}
