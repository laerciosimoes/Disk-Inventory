use jwalk::WalkDirGeneric;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use tauri::ipc::Channel;

use crate::disks::DiskInfo;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::State;
use std::time::Instant;


// Shared application state
#[derive(Default)]
pub struct AppState {
    pub scan_results: Arc<Mutex<HashMap<String, Vec<FsEntry>>>>,
}



#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub path: String,
    pub entry_type: EntryType,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ScanMessage {
    Entries(Vec<FsEntry>),
    Start { total_bytes: u64 },
    Progress { scanned_files: u64, scanned_bytes: u64 },
    Complete,
}
/*
    Skip certain directories during scanning, such as /System/Volumes/Data and any mounted volumes that are not the root path. 
    This is to avoid scanning system directories and other mounted volumes that are not relevant to the current scan.
*/
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

#[tauri::command]
pub fn scan_directory(
    path: String,
    disks: Vec<DiskInfo>,
    channel: Channel<ScanMessage>,
    state: State<'_, AppState>,
) -> Result<(), String> {
scan_directory_internal(path, disks, channel, &state)
}

pub fn scan_directory_internal(
    path: String,
    disks: Vec<DiskInfo>,
    channel: Channel<ScanMessage>,
    state: &AppState,
) -> Result<(), String> {
    let root = Path::new(&path);
    let root_metadata = fs::metadata(root).map_err(|e| e.to_string())?;
    let root_device = root_metadata.dev();

    let mut last_update = Instant::now();

    let total_size_disk: u64 = disks
        .iter()
        .filter(|disk| root.starts_with(&disk.mount_point))
        .map(|disk| disk.total_bytes)
        .sum();
    
    channel.send(ScanMessage::Start { total_bytes: total_size_disk }).ok();

    let exclude_mounted_volumes: Vec<String> = disks
        .iter()
        .filter(|disk| !root.starts_with(&disk.mount_point))
        .map(|disk| disk.mount_point.clone())
        .collect();

    let mut scanned_files = 0u64;
    let mut scanned_bytes = 0u64;

    let mut map: HashMap<String, Vec<FsEntry>> = HashMap::new();
    let mut dir_sizes: HashMap<String, u64> = HashMap::new();

    let root_buf = root.to_path_buf();
    let excludes = exclude_mounted_volumes.clone();

    // Parallel multi-threaded directory walker
    let walker = WalkDirGeneric::<((), u64)>::new(root)
        .skip_hidden(false)
        .process_read_dir(move |_depth, _dir_path, _state, children| {
            // Pre-filter directories across worker threads
            children.retain(|dir_entry_result| {
                if let Ok(entry) = dir_entry_result {
                    let entry_path = entry.path();

                    if entry.file_type.is_dir() &&should_skip_directory(&entry_path, &root_buf, &excludes) {
                        return false;
                    }

                    if let Ok(meta) = entry.metadata() {
                        return meta.dev() == root_device;
                    }
                }
                false
            });
        });

    for entry_result in walker {
        let Ok(entry) = entry_result else { continue };

        let entry_path = entry.path();
        let is_symlink = entry.file_type.is_symlink();
        let is_dir = entry.file_type.is_dir();

        let file_size = if !is_dir && !is_symlink {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        scanned_files += 1;
        scanned_bytes += file_size;
        
        let entry_type = if entry.file_type.is_symlink() {
            EntryType::Symlink
        } else if entry.file_type.is_dir() {
            EntryType::Directory
        } else {
            EntryType::File
        };

        // Group entry under its parent path key
        if let Some(parent) = entry_path.parent() {
            let parent_key = parent.to_string_lossy().into_owned();

            map.entry(parent_key).or_default().push(FsEntry {
                path: entry_path.to_string_lossy().into_owned(),
                entry_type,
                size_bytes: file_size,
            });
        }

        // Roll file size up through ancestor directory paths
        if file_size > 0 {
            let mut curr = entry_path.parent();
            while let Some(p) = curr {
                let p_str = p.to_string_lossy().into_owned();
                *dir_sizes.entry(p_str).or_default() += file_size;
                if p == root {
                    break;
                }
                curr = p.parent();
            }
        }

        // Throttle progress updates to frontend
        if last_update.elapsed().as_millis() >= 500 {
            channel
                .send(ScanMessage::Progress {
                    scanned_files,
                    scanned_bytes,
                })
                .ok();
            last_update = Instant::now();
        }
    }

    // Assign cumulative recursive sizes to Directory entries in the map
    for entries in map.values_mut() {
        for entry in entries.iter_mut() {
            if entry.entry_type == EntryType::Directory {
                if let Some(&size) = dir_sizes.get(&entry.path) {
                    entry.size_bytes = size;
                }
            }
        }
    }

    // Save scan state for lazy querying
    {
        let mut scan_results = state.scan_results.lock().unwrap();
        *scan_results = map;
    }

    // Final notifications
    channel.send(ScanMessage::Progress { scanned_files, scanned_bytes }).ok();
    channel.send(ScanMessage::Complete).ok();

    Ok(())
}


#[tauri::command]
pub fn get_directory_contents(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FsEntry>, String> {
    let map = state.scan_results.lock().unwrap();
    
    // Instantly return only the children of the requested path
    if let Some(children) = map.get(&path) {
        let mut sorted = children.clone();
        // Sort descending by size so largest items are always first
        sorted.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        Ok(sorted)
    } else {
        Ok(Vec::new())
    }
}