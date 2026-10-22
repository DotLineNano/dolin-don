use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PackReport {
    pub input_path: String,
    pub output_path: String,
    pub entries: usize,
    pub stored_bytes: u64,
    pub original_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractReport {
    pub archive_path: String,
    pub output_dir: String,
    pub entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractOneReport {
    pub archive_path: String,
    pub entry_path: String,
    pub output_file: String,
    pub stored_bytes: u64,
    pub original_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ValidateReport {
    pub archive_path: String,
    pub valid: bool,
    pub entries: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveEntryReport {
    pub path: String,
    pub method: String,
    pub crc32: u32,
    pub stored_size: u64,
    pub original_size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveInfoReport {
    pub entry_count: usize,
    pub total_stored_size: u64,
    pub total_original_size: u64,
}

impl ArchiveInfoReport {
    pub fn saved_bytes(&self) -> u64 {
        self.total_original_size
            .saturating_sub(self.total_stored_size)
    }

    pub fn saved_ratio_percent(&self) -> f64 {
        if self.total_original_size == 0 {
            0.0
        } else {
            (self.saved_bytes() as f64 * 100.0) / self.total_original_size as f64
        }
    }
}
