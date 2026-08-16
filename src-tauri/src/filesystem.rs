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
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
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

/// Assigns cumulative recursive sizes onto `Directory` entries in `map`,
/// looking them up from the running `dir_sizes` totals. Used both for the
/// periodic partial flush during a scan and the final backfill, so
/// directories always carry a real size rather than the placeholder `0`
/// they're pushed into `map` with.
fn backfill_directory_sizes(map: &mut HashMap<String, Vec<FsEntry>>, dir_sizes: &HashMap<String, u64>) {
    for entries in map.values_mut() {
        for entry in entries.iter_mut() {
            if entry.entry_type == EntryType::Directory {
                if let Some(&size) = dir_sizes.get(&entry.path) {
                    entry.size_bytes = size;
                }
            }
        }
    }
}

// `async fn` + `spawn_blocking` so the multi-second walk runs on its own OS
// thread instead of inline on the main thread. A plain (non-async) command
// would run its whole body synchronously on the thread that dispatches IPC
// messages, freezing all other commands (including `get_directory_contents`
// polls) and delaying delivery of `channel.send()` messages until the walk
// finishes — see the plan doc for how this was diagnosed.
#[tauri::command]
pub async fn scan_directory(
    path: String,
    disks: Vec<DiskInfo>,
    channel: Channel<ScanMessage>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let scan_results = state.scan_results.clone();
    tauri::async_runtime::spawn_blocking(move || {
        scan_directory_internal(path, disks, channel, scan_results)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub fn scan_directory_internal(
    path: String,
    disks: Vec<DiskInfo>,
    channel: Channel<ScanMessage>,
    scan_results: Arc<Mutex<HashMap<String, Vec<FsEntry>>>>,
) -> Result<(), String> {
    let root = Path::new(&path);
    let root_metadata = fs::metadata(root).map_err(|e| e.to_string())?;
    let root_device = root_metadata.dev();

    let mut last_update = Instant::now();

    let total_size_disk: u64 = disks
        .iter()
        .filter(|disk| root.starts_with(&disk.mount_point))
        .map(|disk| disk.used_bytes)
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

            // Flush a partial snapshot so `get_directory_contents` can serve
            // growing results while the walk is still running, not just
            // after it finishes.
            let mut snapshot = map.clone();
            backfill_directory_sizes(&mut snapshot, &dir_sizes);
            scan_results.lock().unwrap().extend(snapshot);
        }
    }

    // Assign cumulative recursive sizes to Directory entries in the map
    backfill_directory_sizes(&mut map, &dir_sizes);

    // Save scan state for lazy querying. `extend` (not assignment) so a
    // concurrent scan in another volume window can't have its results wiped
    // out by this one finishing.
    scan_results.lock().unwrap().extend(map);

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