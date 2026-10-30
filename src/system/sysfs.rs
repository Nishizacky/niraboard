use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Read a sysfs/procfs file and trim its trailing newline.
pub fn read_string(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
}

/// Read a single typed number from a sysfs/procfs file.
pub fn read_number<T: FromStr>(path: impl AsRef<Path>) -> Option<T> {
    read_string(path)?.parse().ok()
}

/// List directory entries in a stable order.
pub fn entries(path: impl AsRef<Path>) -> Vec<PathBuf> {
    let Ok(directory) = fs::read_dir(path) else {
        return Vec::new();
    };

    let mut entries: Vec<PathBuf> = directory
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    entries
}
