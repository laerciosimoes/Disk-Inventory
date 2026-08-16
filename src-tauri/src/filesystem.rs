use jwalk::WalkDirGeneric;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::Instant,
};

use tauri::{ipc::Channel, State};

use crate::disks::DiskInfo;

// -----------------------------------------------------------------------------
// Shared application state
// -----------------------------------------------------------------------------

#[derive(Default)]
pub struct AppState {
    pub scan_results: Arc<RwLock<ScanIndex>>,
}

// -----------------------------------------------------------------------------
// Scan index
// -----------------------------------------------------------------------------

#[derive(Default)]
pub struct ScanIndex {
    /// Children indexed by their parent directory.
    ///
    /// Example:
    ///
    /// "/Users/laercio" -> [
    ///     "/Users/laercio/Documents",
    ///     "/Users/laercio/file.txt",
    /// ]
    pub entries: HashMap<String, Vec<FsEntry>>,

    /// Recursive size of every directory encountered during the scan.
    ///
    /// Example:
    ///
    /// "/Users"                    -> 120 GB
    /// "/Users/laercio"            -> 80 GB
    /// "/Users/laercio/Documents"  -> 20 GB
    pub directory_sizes: HashMap<String, u64>,

    /// Current scan statistics.
    pub scanned_files: u64,
    pub scanned_bytes: u64,

    /// Whether the current scan has completed.
    pub complete: bool,

    /// Identifies the current scan.
    ///
    /// Useful if multiple scans can be started over the lifetime
    /// of the application.
    pub generation: u64,
}

// -----------------------------------------------------------------------------
// Filesystem entries
// -----------------------------------------------------------------------------

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

// -----------------------------------------------------------------------------
// Messages sent to the frontend
//
// IMPORTANT:
// We intentionally do NOT send individual filesystem entries through
// the Channel. The complete filesystem index remains in Rust.
//
// The frontend receives only scan lifecycle/progress information.
// -----------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum ScanMessage {
    Start {
        total_bytes: u64,
        generation: u64,
    },

    Progress {
        scanned_files: u64,
        scanned_bytes: u64,
        generation: u64,
    },

    Complete {
        generation: u64,
    },
}

// -----------------------------------------------------------------------------
// Response returned by get_directory_contents
// -----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryContents {
    pub path: String,
    pub entries: Vec<FsEntry>,

    /// True when the directory has already been completely scanned.
    pub complete: bool,

    /// Current scan generation.
    pub generation: u64,
}

// -----------------------------------------------------------------------------
// Directory filtering
// -----------------------------------------------------------------------------

/// Skip directories that should not be traversed when scanning the root
/// filesystem.
///
/// When scanning a path other than "/", we do not apply the mounted-volume
/// exclusion logic because the caller explicitly selected that subtree.
fn should_skip_directory(
    path: &Path,
    root: &Path,
    exclude_mounted_volumes: &[String],
) -> bool {
    if root != Path::new("/") {
        return false;
    }

    // Avoid traversing the synthetic Data volume separately when scanning "/".
    if path == Path::new("/System/Volumes/Data") {
        return true;
    }

    exclude_mounted_volumes
        .iter()
        .any(|volume| Path::new(volume) == path)
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Add a file's size to all ancestor directories up to the scan root.
///
/// Example:
///
/// /Users/laercio/Documents/file.pdf
///       100 MB
///
/// contributes to:
///
/// /Users/laercio/Documents -> +100 MB
/// /Users/laercio           -> +100 MB
/// /Users                   -> +100 MB
/// /                       -> +100 MB
fn add_size_to_ancestors(
    entry_path: &Path,
    file_size: u64,
    root: &Path,
    directory_sizes: &mut HashMap<String, u64>,
) {
    if file_size == 0 {
        return;
    }

    let mut current = entry_path.parent();

    while let Some(parent) = current {
        let parent_string = normalize_path(parent);

        *directory_sizes.entry(parent_string).or_insert(0) += file_size;

        if parent == root {
            break;
        }

        current = parent.parent();
    }
}

// -----------------------------------------------------------------------------
// Start scan
// -----------------------------------------------------------------------------

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
    .map_err(|error| error.to_string())?
}

// -----------------------------------------------------------------------------
// Internal scanner
// -----------------------------------------------------------------------------

pub fn scan_directory_internal(
    path: String,
    disks: Vec<DiskInfo>,
    channel: Channel<ScanMessage>,
    scan_results: Arc<RwLock<ScanIndex>>,
) -> Result<(), String> {
    let root = Path::new(&path);

    let root_metadata = fs::metadata(root)
        .map_err(|error| format!("Unable to stat '{}': {}", path, error))?;

    let root_device = root_metadata.dev();

    // -------------------------------------------------------------------------
    // Start a new scan generation
    // -------------------------------------------------------------------------

    let generation = {
        let mut index = scan_results
            .write()
            .map_err(|_| "Failed to acquire scan index lock".to_string())?;

        let generation = index.generation + 1;

        // Clear the previous index.
        //
        // The new scan owns the index from this point forward.
        index.entries.clear();
        index.directory_sizes.clear();
        index.scanned_files = 0;
        index.scanned_bytes = 0;
        index.complete = false;
        index.generation = generation;

        generation
    };

    // -------------------------------------------------------------------------
    // Determine progress denominator
    // -------------------------------------------------------------------------

    let total_size_disk: u64 = disks
        .iter()
        .filter(|disk| root.starts_with(&disk.mount_point))
        .map(|disk| disk.used_bytes)
        .sum();

    channel
        .send(ScanMessage::Start {
            total_bytes: total_size_disk,
            generation,
        })
        .ok();

    // -------------------------------------------------------------------------
    // Mounted-volume exclusions
    // -------------------------------------------------------------------------

    let exclude_mounted_volumes: Vec<String> = disks
        .iter()
        .filter(|disk| !root.starts_with(&disk.mount_point))
        .map(|disk| disk.mount_point.clone())
        .collect();

    let root_buf = root.to_path_buf();
    let excludes = exclude_mounted_volumes.clone();

    // -------------------------------------------------------------------------
    // Local scan state
    //
    // These structures are owned exclusively by the scanner thread.
    // We do not lock the shared state for every filesystem entry.
    // Instead, batches are periodically committed to ScanIndex.
    // -------------------------------------------------------------------------

    let mut local_entries: HashMap<String, Vec<FsEntry>> = HashMap::new();

    let mut local_directory_sizes: HashMap<String, u64> = HashMap::new();

    let mut scanned_files = 0u64;
    let mut scanned_bytes = 0u64;

    let mut last_update = Instant::now();

    // Commit roughly every 250 ms.
    //
    // This is deliberately independent from the progress frequency.
    let commit_interval_ms = 250u128;

    // Progress messages can be sent more frequently.
    let progress_interval_ms = 500u128;

    let mut last_progress = Instant::now();

    // -------------------------------------------------------------------------
    // Parallel jwalk traversal
    // -------------------------------------------------------------------------

    let walker = WalkDirGeneric::<((), u64)>::new(root)
        .skip_hidden(false)
        .process_read_dir(move |_depth, _dir_path, _state, children| {
            children.retain(|dir_entry_result| {
                let Ok(entry) = dir_entry_result else {
                    return false;
                };

                let entry_path = entry.path();

                // Skip unwanted directories.
                if entry.file_type.is_dir()
                    && should_skip_directory(
                        &entry_path,
                        &root_buf,
                        &excludes,
                    )
                {
                    return false;
                }

                // Do not cross filesystem/device boundaries.
                match entry.metadata() {
                    Ok(metadata) => metadata.dev() == root_device,
                    Err(_) => false,
                }
            });
        });

    // -------------------------------------------------------------------------
    // Walk filesystem
    // -------------------------------------------------------------------------

    for entry_result in walker {
        let Ok(entry) = entry_result else {
            continue;
        };

        let entry_path = entry.path();

        let is_symlink = entry.file_type.is_symlink();
        let is_dir = entry.file_type.is_dir();

        let file_size = if !is_dir && !is_symlink {
            entry.metadata()
                .map(|metadata| metadata.len())
                .unwrap_or(0)
        } else {
            0
        };

        let entry_type = if is_symlink {
            EntryType::Symlink
        } else if is_dir {
            EntryType::Directory
        } else {
            EntryType::File
        };

        let entry_path_string = normalize_path(&entry_path);

        let fs_entry = FsEntry {
            path: entry_path_string.clone(),
            entry_type,
            size_bytes: file_size,
        };

        scanned_files += 1;
        scanned_bytes += file_size;

        // ---------------------------------------------------------------------
        // Index child under parent
        // ---------------------------------------------------------------------

        if let Some(parent) = entry_path.parent() {
            let parent_key = normalize_path(parent);

            local_entries
                .entry(parent_key)
                .or_default()
                .push(fs_entry);
        }

        // ---------------------------------------------------------------------
        // Aggregate file size through directory ancestors
        // ---------------------------------------------------------------------

        if file_size > 0 {
            add_size_to_ancestors(
                &entry_path,
                file_size,
                root,
                &mut local_directory_sizes,
            );
        }

        // ---------------------------------------------------------------------
        // Periodic commit to shared state
        // ---------------------------------------------------------------------

        if last_update.elapsed().as_millis() >= commit_interval_ms {
            {
                let mut index = scan_results
                    .write()
                    .map_err(|_| "Failed to acquire scan index lock".to_string())?;

                // Move local entries into the shared index.
                for (parent, entries) in local_entries.drain() {
                    index
                        .entries
                        .entry(parent)
                        .or_default()
                        .extend(entries);
                }

                // Move directory size increments into shared state.
                for (path, size) in local_directory_sizes.drain() {
                    *index
                        .directory_sizes
                        .entry(path)
                        .or_insert(0) += size;
                }

                index.scanned_files = scanned_files;
                index.scanned_bytes = scanned_bytes;
            }

            last_update = Instant::now();
        }

        // ---------------------------------------------------------------------
        // Periodic progress notification
        // ---------------------------------------------------------------------

        if last_progress.elapsed().as_millis() >= progress_interval_ms {
            channel
                .send(ScanMessage::Progress {
                    scanned_files,
                    scanned_bytes,
                    generation,
                })
                .ok();

            last_progress = Instant::now();
        }
    }

    // -------------------------------------------------------------------------
    // Final commit
    // -------------------------------------------------------------------------

    {
        let mut index = scan_results
            .write()
            .map_err(|_| "Failed to acquire scan index lock".to_string())?;

        for (parent, entries) in local_entries.drain() {
            index
                .entries
                .entry(parent)
                .or_default()
                .extend(entries);
        }

        for (path, size) in local_directory_sizes.drain() {
            *index
                .directory_sizes
                .entry(path)
                .or_insert(0) += size;
        }

        // Update directory entry sizes from the final accumulated totals.
        //
        // This is done ONLY once, at the end.
        let directory_sizes = &index.directory_sizes.clone();

        for children in index.entries.values_mut() {
            for entry in children.iter_mut() {
                if entry.entry_type == EntryType::Directory {
                    if let Some(size) = directory_sizes.get(&entry.path) {
                        entry.size_bytes = *size;
                    }
                }
            }
        }

        index.scanned_files = scanned_files;
        index.scanned_bytes = scanned_bytes;
        index.complete = true;
    }

    // -------------------------------------------------------------------------
    // Final progress + completion
    // -------------------------------------------------------------------------

    channel
        .send(ScanMessage::Progress {
            scanned_files,
            scanned_bytes,
            generation,
        })
        .ok();

    channel
        .send(ScanMessage::Complete { generation })
        .ok();

    Ok(())
}

// -----------------------------------------------------------------------------
// Lazy directory query
// -----------------------------------------------------------------------------

#[tauri::command]
pub fn get_directory_contents(
    path: String,
    state: State<'_, AppState>,
) -> Result<DirectoryContents, String> {
    let index = state
        .scan_results
        .read()
        .map_err(|_| "Failed to acquire scan index lock".to_string())?;

    let mut entries = index
        .entries
        .get(&path)
        .cloned()
        .unwrap_or_default();

    // Apply the latest known directory sizes.
    //
    // This is important while a scan is still running. Directory sizes are
    // continuously accumulated in directory_sizes even before the scan
    // completes.
    for entry in &mut entries {
        if entry.entry_type == EntryType::Directory {
            if let Some(size) = index.directory_sizes.get(&entry.path) {
                entry.size_bytes = *size;
            }
        }
    }

    // Largest entries first.
    entries.sort_unstable_by(|a, b| {
        b.size_bytes.cmp(&a.size_bytes)
    });

    Ok(DirectoryContents {
        path,
        entries,
        complete: index.complete,
        generation: index.generation,
    })
}

// -----------------------------------------------------------------------------
// Optional: scan status
//
// This allows the frontend to query the current status without depending
// exclusively on Channel messages.
// -----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub scanning: bool,
    pub scanned_files: u64,
    pub scanned_bytes: u64,
    pub generation: u64,
}

#[tauri::command]
pub fn get_scan_status(
    state: State<'_, AppState>,
) -> Result<ScanStatus, String> {
    let index = state
        .scan_results
        .read()
        .map_err(|_| "Failed to acquire scan index lock".to_string())?;

    Ok(ScanStatus {
        scanning: !index.complete,
        scanned_files: index.scanned_files,
        scanned_bytes: index.scanned_bytes,
        generation: index.generation,
    })
}