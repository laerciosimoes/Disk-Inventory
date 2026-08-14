use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use tauri::ipc::Channel;

use crate::disks::DiskInfo;

const BATCH_SIZE: usize = 1000;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ScanMessage {
    /// Batched entries to eliminate IPC overhead
    Entries(Vec<FsEntry>),
    Progress {
        scanned_files: u64,
        scanned_bytes: u64,
        total_bytes: u64,
    },
    Complete,
}

/// Sums file sizes under `path`, recursing only within `device`
fn dir_size_on_device(path: &Path, device: u64) -> u64 {
    let entries: Vec<_> = match fs::read_dir(path) {
        Ok(read_dir) => read_dir.filter_map(|e| e.ok()).collect(),
        Err(_) => return 0,
    };

    entries
        .into_par_iter()
        .map(|entry| {
            let Ok(file_type) = entry.file_type() else {
                return 0;
            };
            if file_type.is_symlink() {
                return 0;
            }
            let Ok(metadata) = entry.metadata() else {
                return 0;
            };
            if metadata.dev() != device {
                return 0;
            }
            if file_type.is_dir() {
                dir_size_on_device(&entry.path(), device)
            } else {
                metadata.len()
            }
        })
        .sum()
}

pub fn dir_total_size(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(metadata) => dir_size_on_device(path, metadata.dev()),
        Err(_) => 0,
    }
}

fn should_skip_directory(path: &Path, root: &Path, exclude_mounted_volumes: &[String]) -> bool {
    if root != Path::new("/") {
        return false;
    }
    if path == Path::new("/System/Volumes/Data") {
        return true;
    }
    exclude_mounted_volumes
        .iter()
        .any(|v| Path::new(v) == path)
}

fn scan_directory_recursive(
    path: &Path,
    root: &Path,
    root_device: u64,
    channel: &Channel<ScanMessage>,
    exclude_mounted_volumes: &[String],
    total_size_disk: u64,
    scanned_files: &mut u64,
    scanned_bytes: &mut u64,
    batch_buffer: &mut Vec<FsEntry>,
) -> Result<u64, String> {
    let dir = match fs::read_dir(path) {
        Ok(dir) => dir,
        Err(_) => return Ok(0),
    };
    let mut directory_size = 0u64;

    for item in dir {
        let Ok(item) = item else { continue };
        let entry_path = item.path();

        let Ok(file_type) = item.file_type() else { continue };
        let is_symlink = file_type.is_symlink();
        let is_dir = file_type.is_dir();

        let Ok(metadata) = item.metadata() else { continue };

        // Cross-device mount guard
        if metadata.dev() != root_device {
            continue;
        }

        *scanned_files += 1;

        if is_dir && !is_symlink {
            if should_skip_directory(&entry_path, root, exclude_mounted_volumes) {
                continue;
            }

            let sub_size = scan_directory_recursive(
                &entry_path,
                root,
                root_device,
                channel,
                exclude_mounted_volumes,
                total_size_disk,
                scanned_files,
                scanned_bytes,
                batch_buffer,
            )?;

            directory_size += sub_size;

            batch_buffer.push(FsEntry {
                name: item.file_name().to_string_lossy().into_owned(),
                path: entry_path.to_string_lossy().into_owned(),
                is_dir: true,
                is_symlink: false,
                size_bytes: sub_size,
            });
        } else {
            let file_size = if !is_symlink { metadata.len() } else { 0 };

            if !is_symlink {
                *scanned_bytes += file_size;
                directory_size += file_size;
            }

            batch_buffer.push(FsEntry {
                name: item.file_name().to_string_lossy().into_owned(),
                path: entry_path.to_string_lossy().into_owned(),
                is_dir: false,
                is_symlink,
                size_bytes: file_size,
            });
        }

        // Flush batch and send progress when threshold is reached
        if batch_buffer.len() >= BATCH_SIZE {
            channel
                .send(ScanMessage::Entries(std::mem::take(batch_buffer)))
                .map_err(|e| e.to_string())?;

            channel
                .send(ScanMessage::Progress {
                    scanned_files: *scanned_files,
                    scanned_bytes: *scanned_bytes,
                    total_bytes: total_size_disk,
                })
                .map_err(|e| e.to_string())?;
        }
    }

    Ok(directory_size)
}

#[tauri::command]
pub fn scan_directory(
    path: String,
    disks: Vec<DiskInfo>,
    channel: Channel<ScanMessage>,
) -> Result<(), String> {
    let mut scanned_files = 0;
    let mut scanned_bytes = 0;
    let mut batch_buffer = Vec::with_capacity(BATCH_SIZE);

    let root = Path::new(&path);
    let root_metadata = fs::metadata(root).map_err(|e| e.to_string())?;
    let root_device = root_metadata.dev();

    fs::read_dir(root).map_err(|e| e.to_string())?;

    let total_size_disk: u64 = disks
        .iter()
        .filter(|disk| root.starts_with(&disk.mount_point))
        .map(|disk| disk.total_bytes)
        .sum();

    let exclude_mounted_volumes: Vec<String> = disks
        .iter()
        .filter(|disk| !root.starts_with(&disk.mount_point))
        .map(|disk| disk.mount_point.clone())
        .collect();

    let total_size = scan_directory_recursive(
        root,
        root,
        root_device,
        &channel,
        &exclude_mounted_volumes,
        total_size_disk,
        &mut scanned_files,
        &mut scanned_bytes,
        &mut batch_buffer,
    )?;

    // Flush any remaining buffered entries
    if !batch_buffer.is_empty() {
        channel
            .send(ScanMessage::Entries(batch_buffer))
            .map_err(|e| e.to_string())?;
    }

    // Final progress state
    channel
        .send(ScanMessage::Progress {
            scanned_files,
            scanned_bytes: total_size,
            total_bytes: total_size_disk,
        })
        .map_err(|e| e.to_string())?;

    channel
        .send(ScanMessage::Complete)
        .map_err(|e| e.to_string())?;

    Ok(())
}
