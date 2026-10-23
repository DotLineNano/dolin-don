# Dolin

A simple archive tool for `.don` files. Pack a folder into one file, extract
it back, verify integrity. Written in Rust.

Think of it as a mix between `tar` and `zip` — with per-file compression,
chunk-level dedup, and preserved file times.

---

## What it does

- **Pack** a file or folder into a `.don` archive
- **Extract** with file times preserved
- **Verify** every entry with CRC32
- **List** entries without extracting
- **Inspect** summary
- **Benchmark** methods on sample files

One archive can mix different compression methods per file — auto mode picks
what fits best.

---

## Install

Requires Rust 1.70+. No C compiler.

```bash
git clone https://github.com/DotLineNano/dolin-don.git
cd dolin-don
cargo build --release
```

Binary at `target/release/dolin`.

Prebuilt binaries for Linux (x86_64, ARM64), macOS (Intel, Apple Silicon),
and Windows are available in [Releases](https://github.com/DotLineNano/dolin-don/releases/latest).

---

## Basic usage

```bash
dolin pack ./my-photos photos.don            # pack a folder
dolin info photos.don                        # summary
dolin list photos.don                        # list entries
dolin extract photos.don ./restored          # dir must be empty
dolin extract --force photos.don ./restored  # replace non-empty dir
dolin extract --merge photos.don ./restored  # merge into existing dir
dolin validate photos.don                    # verify CRC32
```

In practice, most work is just `pack` and `extract`.

---

## All commands

```
dolin pack [--method <m>] <input> <out.don>     Pack (auto|store|rle|rleext|lz|lzv5|deflate)
dolin info <archive.don>                        Show summary
dolin list [--raw] [--summary] <a.don>          List entries
dolin extract [--force|--merge] <a.don> <dir>   Extract (default: refuse non-empty)
dolin extract-one <a.don> <path> <out>          Extract one file
dolin validate [--quiet] <a.don>                Verify integrity
dolin bench <file_or_dir>                       Benchmark methods
dolin help                                      Show help
```

---

## How compression works

Dolin inspects each file. If it is random, tiny, or already compressed
(APK, PNG, JPEG), it stores as-is. Otherwise it tries a few methods and
keeps the smallest result.

| Method | When picked | Example |
|--------|-------------|---------|
| `stored` | Already compressed, tiny, or random | APK, PNG |
| `deflate` | ZIP containers (APK/ZIP/JAR) | Android apps |
| `rle` | Short runs | Small bitmaps |
| `rleext` | Long runs (up to 64 KB) | Boot images, zero padding |
| `lzv5` | Text, code, binaries | Logs, ELF |
| `lzv5huff` | Text-heavy (LZ + Huffman) | XML, JSON |
| `lz` | Compatibility alias | — |

### Real numbers

Mixed folder (117 files, 90 MB total), ARM64 Termux, 8 cores, release build:

- Original: 90.07 MB
- Archive: 26.69 MB
- Saved: 70.36%
- Time: ~0.36 seconds

A 64 MB boot image packs to **19.99 MB in 0.29 seconds** with `rleext`.

*Numbers from an 8-core ARM64 phone. Yours will differ.*

---

## Format

Single file. No separate index to lose.

```
+------------------------+
| Header         96 B    |  magic DON1, version, offsets, CRC32
| Entry table    68 B*n  |  one per file
| Path table             |  file paths
| Chunk table (v3)       |  for dedup across files
| Data                   |  compressed or raw payloads
+------------------------+
```

v3 is current. v1 and v2 archives still readable.

Stored per file: path, sizes, CRC32, method, mtime.

Not stored (yet): permissions, owner, symlinks, xattr.

---

## Deduplication

- **File-level** — identical CRC32 + size, one payload, second entry
  points to first
- **Chunk-level (v3)** — files over 4 KB split into 4 KB chunks. Identical
  chunks across files stored once. Only for `lzv5`/`lzv5huff`.

---

## Library usage

```rust
use dolin::{pack_path_to_archive, extract_archive_to_dir, get_archive_info};
use std::path::Path;

fn main() -> dolin::Result<()> {
    pack_path_to_archive(Path::new("./data"), Path::new("out.don"))?;
    let info = get_archive_info(Path::new("out.don"))?;
    println!("{} entries, {} bytes", info.entry_count, info.total_stored_size);
    extract_archive_to_dir(Path::new("out.don"), Path::new("./restore"))?;
    Ok(())
}
```

---

## Known issues

- **Permissions not stored.** Extract uses default mode (600/644). Planned
  for format v4.
- **Symlinks not supported.** Extract skips them.
- **Extract is sequential.** Parallel extract planned.
- **`lzv5huff` not always better than `lzv5`.** Auto picks it only if 2%+
  saving.

---

## FAQ

**Do I need to install anything to extract?**
Just the `dolin` binary. No external libraries.

**Is the archive encrypted?**
No. For encryption, wrap the `.don` in an encrypted container.

**Can I use it for backups?**
Yes — file times preserved, dedup keeps repeated data small.

**How does it compare to ZIP?**
Dolin has per-file method selection, chunk-level dedup, and mtime
preservation. ZIP is one algorithm for all files.

**How does it compare to zstd?**
zstd is a raw compressor. Dolin is an archive format — preserves
structure, mtime, dedups across files. Use zstd for raw compression,
Dolin for folders.

**Will .don archives work in the future?**
v3 current. v1 and v2 remain readable. Format changes bump version.

---

## Building

```bash
cargo build --release                        # full binary with JNI
cargo build --release --no-default-features  # CLI only, smaller
```

Dependencies: `crc32fast`, `serde`, `serde_json`, `rayon`, `jni`
(optional). All permissive licenses — see `Cargo.lock`.

---

## Roadmap

- [ ] Store file permissions and owner (format v4)
- [ ] Symlink support
- [ ] Parallel extract
- [ ] `--policy=fast|balanced|max` flag
- [ ] Streaming pack

No dates promised.

---

## License

BSD-3-Clause — see [LICENSE](LICENSE).

Copyright (c) 2026 DotLineNano.

Keep the copyright notice and LICENSE. Do not use the names DOLIN or
DotLineNano to promote derived products without permission.

Forks welcome with attribution.

---

## Links

- Repository: https://github.com/DotLineNano/dolin-don
- Issues: https://github.com/DotLineNano/dolin-don/issues
- License: [BSD-3-Clause](LICENSE)

---
