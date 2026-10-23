use std::env;
use std::path::Path;
use std::time::Instant;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {}", err);
        std::process::exit(1);
    }
}

fn print_usage() {
    eprintln!(
        "\
dolin - simple .don archive tool

USAGE:
dolin pack <input_path> <output.don>
dolin info <archive.don>
dolin list [--raw] [--summary] <archive.don>
dolin extract [--force|--merge] <archive.don> <output_dir>
dolin extract-one <archive.don> <entry_path> <output_file>
dolin validate [--quiet] <archive.don>
dolin bench <file_or_dir>
dolin help

COMMANDS:
pack         Create a .don archive from a file or directory
info         Show archive summary
list         List archive entries
extract      Extract all entries (default: refuse non-empty dir)
extract-one  Extract one entry into a file
validate     Verify archive structure and CRC32 integrity
bench        Compare raw/RLE/LZ sizes on a file or directory
help         Show this help message

OPTIONS:
  --force      [extract] Replace non-empty output directory
  --merge      [extract] Merge into existing directory (tar/unzip style)
  --raw        Use machine-friendly output for 'list'
  --summary    Show archive totals after 'list'
  --quiet      Suppress success output for 'validate'
"
    );
}

fn run() -> dolin::Result<()> {
    let mut args = env::args().skip(1);

    let Some(command) = args.next() else {
        print_usage();
        return Ok(());
    };

    match command.as_str() {
        "pack" => {
            // Parse optional --method flag first, then positional args
            let mut method_str: Option<String> = None;
            let mut positional: Vec<String> = Vec::new();

            while positional.len() < 2 {
                let arg = match args.next() {
                    Some(a) => a,
                    None => break,
                };
                match arg.as_str() {
                    "--method" => {
                        method_str = args.next();
                        if method_str.is_none() {
                            eprintln!("error: --method requires a value");
                            std::process::exit(1);
                        }
                    }
                    _ if arg.starts_with("--method=") => {
                        method_str = Some(arg.trim_start_matches("--method=").to_string());
                    }
                    _ => positional.push(arg),
                }
            }

            if positional.len() < 2 {
                eprintln!("error: missing arguments");
                eprintln!("usage: dolin pack [--method <name>] <input_path> <output.don>");
                std::process::exit(1);
            }

            let input = &positional[0];
            let output = &positional[1];

            let method = match method_str.as_deref() {
                Some(s) => match dolin::PackMethod::parse(s) {
                    Some(m) => m,
                    None => {
                        eprintln!("error: unknown method: {}", s);
                        eprintln!("valid: auto|store|rle|rleext|lz|lzv5|deflate");
                        std::process::exit(1);
                    }
                },
                None => dolin::PackMethod::Auto,
            };

            dolin::pack_path_to_archive_with_method(
                Path::new(input),
                Path::new(output),
                method,
            )?;
            let info = dolin::get_archive_info(Path::new(output))?;

            println!("packed:   {} -> {}", input, output);
            println!("entries:  {}", info.entry_count);
            println!(
                "stored:   {} ({})",
                info.total_stored_size,
                human_size(info.total_stored_size)
            );
            println!(
                "original: {} ({})",
                info.total_original_size,
                human_size(info.total_original_size)
            );
        }

        "info" => {
            let archive = require_arg(&mut args, "info <archive.don>")?;

            let info = dolin::get_archive_info(Path::new(&archive))?;
            let saved = info
                .total_original_size
                .saturating_sub(info.total_stored_size);

            println!("archive:   {}", archive);
            println!("entries:   {}", info.entry_count);
            println!(
                "stored:    {} ({})",
                info.total_stored_size,
                human_size(info.total_stored_size)
            );
            println!(
                "original:  {} ({})",
                info.total_original_size,
                human_size(info.total_original_size)
            );
            println!("saved:     {} ({})", saved, human_size(saved));

            if info.total_original_size > 0 {
                let saved_ratio = (saved as f64 * 100.0) / info.total_original_size as f64;
                println!("ratio:     {:.2}% saved", saved_ratio);
            } else {
                println!("ratio:     0.00% saved");
            }
        }

        "list" => {
            let mut raw = false;
            let mut summary = false;
            let mut archive: Option<String> = None;

            for arg in args {
                match arg.as_str() {
                    "--raw" => raw = true,
                    "--summary" => summary = true,
                    _ if archive.is_none() => archive = Some(arg),
                    _ => {
                        eprintln!("error: unexpected argument '{}'", arg);
                        eprintln!("usage: dolin list [--raw] [--summary] <archive.don>");
                        std::process::exit(1);
                    }
                }
            }

            let archive = match archive {
                Some(v) => v,
                None => {
                    print_usage();
                    return Ok(());
                }
            };

            let entries = dolin::list_archive_entries(Path::new(&archive))?;

            if raw {
                for (idx, entry) in entries.iter().enumerate() {
                    println!(
                        "{}\t{}\t{:08x}\t{}\t{}\t{}",
                        idx + 1,
                        entry.method,
                        entry.crc32,
                        entry.stored_size,
                        entry.original_size,
                        entry.path
                    );
                }

                if summary {
                    let info = dolin::get_archive_info(Path::new(&archive))?;
                    let saved = info
                        .total_original_size
                        .saturating_sub(info.total_stored_size);

                    println!("summary");
                    println!("entries\t{}", info.entry_count);
                    println!("stored\t{}", info.total_stored_size);
                    println!("original\t{}", info.total_original_size);
                    println!("saved\t{}", saved);

                    if info.total_original_size > 0 {
                        let saved_ratio = (saved as f64 * 100.0) / info.total_original_size as f64;
                        println!("saved_ratio\t{:.2}", saved_ratio);
                    } else {
                        println!("saved_ratio\t0.00");
                    }
                }

                return Ok(());
            }

            println!("archive: {}", archive);

            if entries.is_empty() {
                println!("(empty)");
            } else {
                println!(
                    "{:<4}  {:<8}  {:<8}  {:>10}  {:>10}  {}",
                    "#", "method", "crc32", "stored", "original", "path"
                );

                for (idx, entry) in entries.iter().enumerate() {
                    println!(
                        "{:<4}  {:<8}  {:08x}  {:>10}  {:>10}  {}",
                        idx + 1,
                        method_name(entry.method),
                        entry.crc32,
                        human_size(entry.stored_size),
                        human_size(entry.original_size),
                        entry.path
                    );
                }
            }

            if summary {
                let info = dolin::get_archive_info(Path::new(&archive))?;
                let saved = info
                    .total_original_size
                    .saturating_sub(info.total_stored_size);

                println!();
                println!("summary:");
                println!("entries:   {}", info.entry_count);
                println!(
                    "stored:    {} ({})",
                    info.total_stored_size,
                    human_size(info.total_stored_size)
                );
                println!(
                    "original:  {} ({})",
                    info.total_original_size,
                    human_size(info.total_original_size)
                );
                println!("saved:     {} ({})", saved, human_size(saved));

                if info.total_original_size > 0 {
                    let saved_ratio = (saved as f64 * 100.0) / info.total_original_size as f64;
                    println!("ratio:     {:.2}% saved", saved_ratio);
                } else {
                    println!("ratio:     0.00% saved");
                }
            }
        }
        "extract" => {
            let mut force = false;
            let mut merge = false;
            let mut positional: Vec<String> = Vec::new();

            while positional.len() < 2 {
                let arg = match args.next() {
                    Some(a) => a,
                    None => break,
                };
                match arg.as_str() {
                    "--force" | "-f" => force = true,
                    "--merge" | "-m" => merge = true,
                    _ => positional.push(arg),
                }
            }

            if force && merge {
                eprintln!("error: --force and --merge are mutually exclusive");
                std::process::exit(1);
            }

            if positional.len() < 2 {
                eprintln!("error: missing arguments");
                eprintln!("usage: dolin extract [--force|--merge] <archive.don> <output_dir>");
                std::process::exit(1);
            }

            let archive = &positional[0];
            let output_dir = &positional[1];

            let entries = dolin::list_archive_entries(Path::new(archive))?;
            let opts = dolin::ExtractOptions { overwrite: force, merge };
            dolin::extract_archive_to_dir_with(Path::new(archive), Path::new(output_dir), opts)?;

            println!("extracted: {} -> {}", archive, output_dir);
            println!("entries:   {}", entries.len());
        }

        "extract-one" => {
            let archive = require_arg(
                &mut args,
                "extract-one <archive.don> <entry_path> <output_file>",
            )?;
            let entry_path = require_arg(
                &mut args,
                "extract-one <archive.don> <entry_path> <output_file>",
            )?;
            let output_file = require_arg(
                &mut args,
                "extract-one <archive.don> <entry_path> <output_file>",
            )?;

            dolin::extract_archive_entry(
                Path::new(&archive),
                &entry_path,
                Path::new(&output_file),
            )?;
            println!("extracted: {}:{} -> {}", archive, entry_path, output_file);
        }

        "validate" => {
            let mut quiet = false;
            let next = args.next();

            let archive = match next.as_deref() {
                Some("--quiet") => {
                    quiet = true;
                    require_arg(&mut args, "validate [--quiet] <archive.don>")?
                }
                Some(value) => value.to_string(),
                None => {
                    print_usage();
                    return Ok(());
                }
            };

            dolin::validate_archive(Path::new(&archive))?;
            if !quiet {
                println!("valid: {}", archive);
            }
        }

        "bench" => {
            let input = require_arg(&mut args, "bench <file_or_dir>")?;
            run_bench(Path::new(&input))?;
        }

        "-h" | "--help" | "help" => {
            print_usage();
        }

        other => {
            eprintln!("error: unknown command '{}'", other);
            eprintln!();
            print_usage();
            std::process::exit(1);
        }
    }

    Ok(())
}

fn require_arg(args: &mut impl Iterator<Item = String>, usage: &str) -> dolin::Result<String> {
    match args.next() {
        Some(value) => Ok(value),
        None => {
            eprintln!("error: missing argument");
            eprintln!("usage: dolin {}", usage);
            std::process::exit(1);
        }
    }
}

fn method_name(method: u16) -> &'static str {
    match method {
        0 => "store",
        1 => "rle",
        2 => "lz",
        3 => "lzv5",
        4 => "lzv5ro",
        5 => "lzv5huff",
        6 => "rleext",
        7 => "deflate",
        _ => "unknown",
    }
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0usize;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.2} {}", value, UNITS[unit])
    }
}

fn run_bench(path: &Path) -> dolin::Result<()> {
    let mut files = Vec::new();
    collect_files(path, &mut files)?;

    if files.is_empty() {
        eprintln!("error: no files found");
        std::process::exit(1);
    }

    let mut total_raw = 0u64;
    let mut total_rle = 0u64;
    let mut total_lz = 0u64;
    let mut total_best = 0u64;

    let mut count_store = 0u64;
    let mut count_rle = 0u64;
    let mut count_lz = 0u64;
    let mut count_lzv5 = 0u64;

    let mut nanos_rle = 0u128;
    let mut nanos_lz = 0u128;
    let mut nanos_lzv5 = 0u128;

    println!(
        "{:<4}  {:>10}  {:>10}  {:>10}  {:>10}  {:>10}  {:<6}  {}",
        "#", "raw", "rle", "lz", "lzv5", "best", "win", "path"
    );

    for (idx, file) in files.iter().enumerate() {
        let data = std::fs::read(file)?;
        let raw = data.len() as u64;

        let t0 = Instant::now();
        let rle = dolin::bench_rle_size(&data) as u64;
        nanos_rle += t0.elapsed().as_nanos();

        let t1 = Instant::now();
        let lz = dolin::bench_lz_size(&data) as u64;
        nanos_lz += t1.elapsed().as_nanos();

        let t2 = Instant::now();
        let lzv5 = dolin::bench_lz_v5_size(&data) as u64;
        nanos_lzv5 += t2.elapsed().as_nanos();

        let mut best = raw;
        let mut win = "store";

        if rle < best {
            best = rle;
            win = "rle";
        }

        if lz < best {
            best = lz;
            win = "lz";
        }

        if lzv5 < best {
            best = lzv5;
            win = "lzv5";
        }

        match win {
            "store" => count_store += 1,
            "rle" => count_rle += 1,
            "lz" => count_lz += 1,
            "lzv5" => count_lzv5 += 1,
            _ => {}
        }

        total_raw += raw;
        total_rle += rle;
        total_lz += lz;
        total_best += best;

        let shown = file.strip_prefix(path).unwrap_or(file);

        println!(
            "{:<4}  {:>10}  {:>10}  {:>10}  {:>10}  {:>10}  {:<6}  {}",
            idx + 1,
            raw,
            rle,
            lz,
            lzv5,
            best,
            win,
            shown.display()
        );
    }

    println!();
    println!("summary:");
    println!("files:      {}", files.len());
    println!("raw:        {} ({})", total_raw, human_size(total_raw));
    println!("rle:        {} ({})", total_rle, human_size(total_rle));
    println!("lz:         {} ({})", total_lz, human_size(total_lz));
    println!("best:       {} ({})", total_best, human_size(total_best));

    let saved = total_raw.saturating_sub(total_best);
    println!("saved:      {} ({})", saved, human_size(saved));

    if total_raw > 0 {
        let ratio = (saved as f64 * 100.0) / total_raw as f64;
        println!("saved ratio:{:.2}%", ratio);
    } else {
        println!("saved ratio:0.00%");
    }

    println!("winner:");
    println!("  store: {}", count_store);
    println!("  rle:   {}", count_rle);
    println!("  lz:    {}", count_lz);
    println!("  lzv5:  {}", count_lzv5);

    println!("timing:");
    println!("  rle:   {:.3} ms", nanos_rle as f64 / 1_000_000.0);
    println!("  lz:    {:.3} ms", nanos_lz as f64 / 1_000_000.0);
    println!("  lzv5:  {:.3} ms", nanos_lzv5 as f64 / 1_000_000.0);

    Ok(())
}

fn collect_files(path: &Path, out: &mut Vec<std::path::PathBuf>) -> dolin::Result<()> {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if name == "target" || name == ".git" {
        return Ok(());
    }

    let meta = std::fs::metadata(path)?;

    if meta.is_file() {
        let skip = name.ends_with(".bak")
            || name.ends_with(".prev")
            || name.ends_with(".tmp")
            || name.ends_with(".part")
            || name.ends_with('~');

        if !skip {
            out.push(path.to_path_buf());
        }
        return Ok(());
    }

    if meta.is_dir() {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)? {
            entries.push(entry?.path());
        }
        entries.sort();

        for entry in entries {
            collect_files(&entry, out)?;
        }
    }

    Ok(())
}
