use std::fs::File;
use std::io::{Read, Take};
use std::path::{Component, Path};

use crc32fast::Hasher;

use crate::error::{DolinError, Result};

pub fn sanitize_relative_path(path: &Path) -> Result<String> {
    if path.is_absolute() {
        return Err(DolinError::InvalidPath(path.display().to_string()));
    }

    let mut parts = Vec::new();
    for comp in path.components() {
        match comp {
            Component::Normal(v) => {
                let s = v.to_string_lossy();
                if s.is_empty() {
                    return Err(DolinError::InvalidPath(path.display().to_string()));
                }
                parts.push(s.to_string());
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(DolinError::InvalidPath(path.display().to_string()));
            }
        }
    }

    if parts.is_empty() {
        return Err(DolinError::InvalidPath(path.display().to_string()));
    }

    Ok(parts.join("/"))
}

pub fn crc32_reader<R: Read>(reader: &mut R) -> Result<u32> {
    let mut hasher = Hasher::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hasher.finalize())
}

pub fn crc32_file(path: &Path) -> Result<u32> {
    let mut file = File::open(path)?;
    crc32_reader(&mut file)
}

pub fn crc32_take(reader: &mut Take<&mut File>) -> Result<u32> {
    crc32_reader(reader)
}
