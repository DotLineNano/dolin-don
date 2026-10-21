use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::format::{reader, validate, writer};

#[derive(Debug, Clone)]
pub struct DonEntryInfo {
    pub path: String,
    pub stored_size: u64,
    pub original_size: u64,
    pub crc32: u32,
    pub method: u16,
    pub mtime_unix: u64,
}

#[derive(Debug, Clone)]
pub struct DonArchiveInfo {
    pub entry_count: u32,
    pub total_stored_size: u64,
    pub total_original_size: u64,
}

#[derive(Debug, Clone)]
pub struct DonArchive {
    pub path: PathBuf,
    pub info: DonArchiveInfo,
    pub entries: Vec<DonEntryInfo>,
}

impl DonArchive {
    pub fn open(path: &Path) -> Result<Self> {
        let entries = reader::read_archive_entries(path)?;
        let total_stored_size = entries.iter().map(|e| e.stored_size).sum();
        let total_original_size = entries.iter().map(|e| e.original_size).sum();

        let info = DonArchiveInfo {
            entry_count: entries.len() as u32,
            total_stored_size,
            total_original_size,
        };

        Ok(Self {
            path: path.to_path_buf(),
            info,
            entries,
        })
    }

    pub fn entries(&self) -> &[DonEntryInfo] {
        &self.entries
    }

    pub fn info(&self) -> &DonArchiveInfo {
        &self.info
    }
}

pub fn pack_path_to_archive(input_path: &Path, output_file: &Path) -> Result<()> {
    writer::pack_path_to_archive(input_path, output_file)
}

pub fn list_archive_entries(archive_path: &Path) -> Result<Vec<DonEntryInfo>> {
    reader::read_archive_entries(archive_path)
}

pub fn get_archive_info(archive_path: &Path) -> Result<DonArchiveInfo> {
    let archive = DonArchive::open(archive_path)?;
    Ok(archive.info)
}

pub fn extract_archive_to_dir(archive_path: &Path, output_dir: &Path) -> Result<()> {
    reader::extract_archive_to_dir(archive_path, output_dir)
}

pub fn extract_archive_entry(
    archive_path: &Path,
    entry_path: &str,
    output_file: &Path,
) -> Result<()> {
    reader::extract_archive_entry(archive_path, entry_path, output_file)
}

pub fn validate_archive(archive_path: &Path) -> Result<()> {
    validate::validate_archive(archive_path)
}


use crate::report::{
    ArchiveEntryReport,
    ArchiveInfoReport,
    ExtractOneReport,
    ExtractReport,
    PackReport,
    ValidateReport,
};

fn report_method_to_string<M: core::fmt::Debug>(method: M) -> String {
    format!("{:?}", method).to_lowercase()
}

pub fn archive_info_report(path: &Path) -> Result<ArchiveInfoReport> {
    let info = get_archive_info(path)?;
    Ok(ArchiveInfoReport {
        entry_count: info.entry_count as usize,
        total_stored_size: info.total_stored_size,
        total_original_size: info.total_original_size,
    })
}

pub fn list_archive_report(path: &Path) -> Result<Vec<ArchiveEntryReport>> {
    let entries = list_archive_entries(path)?;
    Ok(entries
        .into_iter()
        .map(|e| ArchiveEntryReport {
            path: e.path,
            method: report_method_to_string(e.method),
            crc32: e.crc32,
            stored_size: e.stored_size,
            original_size: e.original_size,
        })
        .collect())
}

pub fn pack_path_report(input: &Path, output: &Path) -> Result<PackReport> {
    pack_path_to_archive(input, output)?;
    let info = archive_info_report(output)?;

    Ok(PackReport {
        input_path: input.display().to_string(),
        output_path: output.display().to_string(),
        entries: info.entry_count,
        stored_bytes: info.total_stored_size,
        original_bytes: info.total_original_size,
    })
}

pub fn extract_archive_report(path: &Path, out_dir: &Path) -> Result<ExtractReport> {
    extract_archive_to_dir(path, out_dir)?;
    let entries = list_archive_report(path)?.len();

    Ok(ExtractReport {
        archive_path: path.display().to_string(),
        output_dir: out_dir.display().to_string(),
        entries,
    })
}

pub fn extract_archive_entry_report(
    path: &Path,
    entry_path: &str,
    output_file: &Path,
) -> Result<ExtractOneReport> {
    let entries = list_archive_report(path)?;
    let meta = entries.iter().find(|e| e.path == entry_path).cloned();

    extract_archive_entry(path, entry_path, output_file)?;

    let (stored_bytes, original_bytes) = match meta {
        Some(m) => (m.stored_size, m.original_size),
        None => (0, 0),
    };

    Ok(ExtractOneReport {
        archive_path: path.display().to_string(),
        entry_path: entry_path.to_string(),
        output_file: output_file.display().to_string(),
        stored_bytes,
        original_bytes,
    })
}

pub fn validate_archive_report(path: &Path) -> Result<ValidateReport> {
    validate_archive(path)?;
    let info = archive_info_report(path)?;

    Ok(ValidateReport {
        archive_path: path.display().to_string(),
        valid: true,
        entries: info.entry_count,
    })
}


pub fn archive_info_report_json(path: &Path) -> Result<String> {
    let report = archive_info_report(path)?;
    serde_json::to_string(&report).map_err(|e| e.into())
}

pub fn list_archive_report_json(path: &Path) -> Result<String> {
    let report = list_archive_report(path)?;
    serde_json::to_string(&report).map_err(|e| e.into())
}

pub fn pack_path_report_json(input: &Path, output: &Path) -> Result<String> {
    let report = pack_path_report(input, output)?;
    serde_json::to_string(&report).map_err(|e| e.into())
}

pub fn extract_archive_report_json(path: &Path, out_dir: &Path) -> Result<String> {
    let report = extract_archive_report(path, out_dir)?;
    serde_json::to_string(&report).map_err(|e| e.into())
}

pub fn extract_archive_entry_report_json(
    path: &Path,
    entry_path: &str,
    output_file: &Path,
) -> Result<String> {
    let report = extract_archive_entry_report(path, entry_path, output_file)?;
    serde_json::to_string(&report).map_err(|e| e.into())
}

pub fn validate_archive_report_json(path: &Path) -> Result<String> {
    let report = validate_archive_report(path)?;
    serde_json::to_string(&report).map_err(|e| e.into())
}
