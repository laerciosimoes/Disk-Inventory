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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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

/// Builds the lazy per-level directory listing from an already-locked
/// `ScanIndex`.
///
/// Split out of `get_directory_contents` so this logic can be unit tested
/// directly, without going through Tauri's managed `State` extractor.
fn build_directory_contents(path: String, index: &ScanIndex) -> DirectoryContents {
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

    DirectoryContents {
        path,
        entries,
        complete: index.complete,
        generation: index.generation,
    }
}

#[tauri::command]
pub fn get_directory_contents(
    path: String,
    state: State<'_, AppState>,
) -> Result<DirectoryContents, String> {
    let index = state
        .scan_results
        .read()
        .map_err(|_| "Failed to acquire scan index lock".to_string())?;

    Ok(build_directory_contents(path, &index))
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Serialization contract
    //
    // The frontend types in `src/types.ts` assume `#[serde(rename_all =
    // "camelCase")]` on these structs; these tests lock that contract in
    // directly rather than relying on it only being exercised incidentally.
    // -------------------------------------------------------------------------

    #[test]
    fn app_state_defaults_to_an_empty_scan_index() {
        let state = AppState::default();
        let index = state.scan_results.read().unwrap();

        assert!(index.entries.is_empty());
        assert!(!index.complete);
        assert_eq!(index.generation, 0);
    }

    #[test]
    fn fs_entry_serializes_with_camel_case_keys() {
        let entry = FsEntry {
            path: "/Users/laercio/file.txt".to_string(),
            entry_type: EntryType::File,
            size_bytes: 42,
        };

        let json = serde_json::to_string(&entry).unwrap();

        assert!(json.contains("\"entryType\":\"file\""));
        assert!(json.contains("\"sizeBytes\":42"));
    }

    #[test]
    fn directory_contents_serializes_with_camel_case_keys() {
        let contents = DirectoryContents {
            path: "/Users/laercio".to_string(),
            entries: vec![],
            complete: true,
            generation: 3,
        };

        let json = serde_json::to_string(&contents).unwrap();

        assert!(json.contains("\"complete\":true"));
        assert!(json.contains("\"generation\":3"));
    }

    #[test]
    fn scan_message_tags_variants_by_type_with_nested_data() {
        let start = ScanMessage::Start {
            total_bytes: 100,
            generation: 1,
        };

        let json = serde_json::to_string(&start).unwrap();

        assert!(json.contains("\"type\":\"start\""));
        assert!(json.contains("\"data\":"));
        assert!(json.contains("\"totalBytes\":100"));

        let round_tripped: ScanMessage = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            round_tripped,
            ScanMessage::Start {
                total_bytes: 100,
                generation: 1
            }
        ));
    }

    // -------------------------------------------------------------------------
    // normalize_path
    // -------------------------------------------------------------------------

    #[test]
    fn normalize_path_stringifies_a_path() {
        assert_eq!(
            normalize_path(Path::new("/Users/laercio/Documents")),
            "/Users/laercio/Documents"
        );
    }

    // -------------------------------------------------------------------------
    // should_skip_directory
    // -------------------------------------------------------------------------

    #[test]
    fn never_skips_when_scanning_a_subtree_other_than_the_filesystem_root() {
        let root = Path::new("/Users/laercio");
        let excludes = vec!["/Volumes/Backup".to_string()];

        // Even a path that would normally be excluded is left alone, because
        // the caller explicitly chose to scan this subtree.
        assert!(!should_skip_directory(
            Path::new("/Volumes/Backup"),
            root,
            &excludes
        ));
    }

    #[test]
    fn skips_the_synthetic_data_volume_when_scanning_the_filesystem_root() {
        let root = Path::new("/");

        assert!(should_skip_directory(
            Path::new("/System/Volumes/Data"),
            root,
            &[]
        ));
    }

    #[test]
    fn skips_other_mounted_volumes_when_scanning_the_filesystem_root() {
        let root = Path::new("/");
        let excludes = vec!["/Volumes/Backup".to_string()];

        assert!(should_skip_directory(
            Path::new("/Volumes/Backup"),
            root,
            &excludes
        ));
        assert!(!should_skip_directory(
            Path::new("/Volumes/Other"),
            root,
            &excludes
        ));
    }

    // -------------------------------------------------------------------------
    // add_size_to_ancestors
    // -------------------------------------------------------------------------

    #[test]
    fn zero_sized_files_do_not_touch_the_map() {
        let mut sizes = HashMap::new();

        add_size_to_ancestors(
            Path::new("/Users/laercio/file.txt"),
            0,
            Path::new("/Users"),
            &mut sizes,
        );

        assert!(sizes.is_empty());
    }

    #[test]
    fn adds_file_size_to_every_ancestor_up_to_the_root_inclusive() {
        let mut sizes = HashMap::new();
        let root = Path::new("/Users");

        add_size_to_ancestors(
            Path::new("/Users/laercio/Documents/file.pdf"),
            100,
            root,
            &mut sizes,
        );

        assert_eq!(sizes.get("/Users/laercio/Documents"), Some(&100));
        assert_eq!(sizes.get("/Users/laercio"), Some(&100));
        assert_eq!(sizes.get("/Users"), Some(&100));

        // The root's own parent ("/") is not touched: the walk stops once
        // it reaches `root`.
        assert_eq!(sizes.len(), 3);
    }

    #[test]
    fn accumulates_sizes_from_multiple_files_under_the_same_ancestor() {
        let mut sizes = HashMap::new();
        let root = Path::new("/Users");

        add_size_to_ancestors(Path::new("/Users/laercio/a.txt"), 50, root, &mut sizes);
        add_size_to_ancestors(Path::new("/Users/laercio/b.txt"), 30, root, &mut sizes);

        assert_eq!(sizes.get("/Users/laercio"), Some(&80));
        assert_eq!(sizes.get("/Users"), Some(&80));
    }

    // -------------------------------------------------------------------------
    // build_directory_contents
    // -------------------------------------------------------------------------

    fn file(path: &str, size: u64) -> FsEntry {
        FsEntry {
            path: path.to_string(),
            entry_type: EntryType::File,
            size_bytes: size,
        }
    }

    fn dir(path: &str, size: u64) -> FsEntry {
        FsEntry {
            path: path.to_string(),
            entry_type: EntryType::Directory,
            size_bytes: size,
        }
    }

    #[test]
    fn returns_an_empty_listing_for_an_unknown_path() {
        let index = ScanIndex::default();

        let result = build_directory_contents("/nowhere".to_string(), &index);

        assert_eq!(result.path, "/nowhere");
        assert!(result.entries.is_empty());
    }

    #[test]
    fn sorts_entries_largest_first() {
        let mut index = ScanIndex::default();
        index.entries.insert(
            "/Users/laercio".to_string(),
            vec![
                file("/Users/laercio/small.txt", 10),
                file("/Users/laercio/big.txt", 1000),
            ],
        );

        let result = build_directory_contents("/Users/laercio".to_string(), &index);

        assert_eq!(result.entries[0].path, "/Users/laercio/big.txt");
        assert_eq!(result.entries[1].path, "/Users/laercio/small.txt");
    }

    #[test]
    fn patches_directory_entries_with_the_latest_running_size() {
        let mut index = ScanIndex::default();
        index
            .entries
            .insert("/Users".to_string(), vec![dir("/Users/laercio", 0)]);
        index
            .directory_sizes
            .insert("/Users/laercio".to_string(), 4096);

        let result = build_directory_contents("/Users".to_string(), &index);

        assert_eq!(result.entries[0].size_bytes, 4096);
    }

    #[test]
    fn leaves_file_sizes_untouched_by_directory_size_patching() {
        let mut index = ScanIndex::default();
        index
            .entries
            .insert("/Users".to_string(), vec![file("/Users/notes.txt", 123)]);
        index
            .directory_sizes
            .insert("/Users/notes.txt".to_string(), 999);

        let result = build_directory_contents("/Users".to_string(), &index);

        assert_eq!(result.entries[0].size_bytes, 123);
    }

    #[test]
    fn forwards_the_index_completion_and_generation() {
        let mut index = ScanIndex::default();
        index.complete = true;
        index.generation = 7;

        let result = build_directory_contents("/anything".to_string(), &index);

        assert!(result.complete);
        assert_eq!(result.generation, 7);
    }

    // -------------------------------------------------------------------------
    // scan_directory_internal
    //
    // These exercise the real jwalk traversal against a real (temporary)
    // directory tree, so they cover the walker/commit/message-emission logic
    // that the pure helpers above can't reach on their own.
    // -------------------------------------------------------------------------

    use std::sync::Mutex;

    /// Minimal self-cleaning temp-directory helper, to avoid pulling in the
    /// `tempfile` crate for a single test module.
    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "disk-inventory-test-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));

            fs::create_dir_all(&path).unwrap();

            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write_file(&self, relative: &str, contents: &[u8]) {
            let full = self.path.join(relative);

            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }

            fs::write(full, contents).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    /// A `Channel` that records every message sent through it, so tests can
    /// assert on the scan's lifecycle messages.
    fn collecting_channel() -> (Channel<ScanMessage>, Arc<Mutex<Vec<ScanMessage>>>) {
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_for_channel = received.clone();

        let channel = Channel::new(move |message| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = message {
                if let Ok(msg) = serde_json::from_str::<ScanMessage>(&json) {
                    received_for_channel.lock().unwrap().push(msg);
                }
            }
            Ok(())
        });

        (channel, received)
    }

    #[test]
    fn scans_a_small_tree_and_records_entries_and_sizes() {
        let temp = TempDir::new("scan-basic");
        temp.write_file("a.txt", b"hello"); // 5 bytes
        temp.write_file("sub/b.txt", b"world!"); // 6 bytes

        let root = temp.path().to_string_lossy().into_owned();

        let (channel, received) = collecting_channel();
        let scan_results = Arc::new(RwLock::new(ScanIndex::default()));

        let result = scan_directory_internal(root.clone(), vec![], channel, scan_results.clone());

        assert!(result.is_ok());

        let index = scan_results.read().unwrap();
        assert!(index.complete);
        assert_eq!(index.generation, 1);
        assert_eq!(index.scanned_bytes, 11);

        let root_children = index.entries.get(&root).expect("root has children");
        let names: Vec<&str> = root_children
            .iter()
            .map(|e| Path::new(&e.path).file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));

        let sub_path = format!("{root}/sub");
        let sub_children = index.entries.get(&sub_path).expect("sub has children");
        assert_eq!(sub_children.len(), 1);
        assert_eq!(sub_children[0].size_bytes, 6);

        assert_eq!(index.directory_sizes.get(&sub_path), Some(&6));
        assert_eq!(index.directory_sizes.get(&root), Some(&11));

        let messages = received.lock().unwrap();
        assert!(matches!(
            messages.first(),
            Some(ScanMessage::Start { generation: 1, .. })
        ));
        assert!(matches!(
            messages.last(),
            Some(ScanMessage::Complete { generation: 1 })
        ));
    }

    #[test]
    fn a_second_scan_increments_generation_and_replaces_the_index() {
        let temp_a = TempDir::new("scan-gen-a");
        temp_a.write_file("one.txt", b"x");

        let temp_b = TempDir::new("scan-gen-b");
        temp_b.write_file("two.txt", b"yy");

        let path_a = temp_a.path().to_string_lossy().into_owned();
        let path_b = temp_b.path().to_string_lossy().into_owned();

        let scan_results = Arc::new(RwLock::new(ScanIndex::default()));

        let (channel_a, _) = collecting_channel();
        scan_directory_internal(path_a.clone(), vec![], channel_a, scan_results.clone()).unwrap();

        let (channel_b, received_b) = collecting_channel();
        scan_directory_internal(path_b.clone(), vec![], channel_b, scan_results.clone()).unwrap();

        let index = scan_results.read().unwrap();
        assert_eq!(index.generation, 2);
        assert!(!index.entries.contains_key(&path_a));
        assert!(index.entries.contains_key(&path_b));

        let messages = received_b.lock().unwrap();
        assert!(matches!(
            messages.first(),
            Some(ScanMessage::Start { generation: 2, .. })
        ));
    }

    #[test]
    fn returns_an_error_for_a_path_that_does_not_exist() {
        let (channel, _) = collecting_channel();
        let scan_results = Arc::new(RwLock::new(ScanIndex::default()));

        let result = scan_directory_internal(
            "/definitely/does/not/exist/disk-inventory-test".to_string(),
            vec![],
            channel,
            scan_results,
        );

        assert!(result.is_err());
    }

    #[test]
    fn start_message_totals_the_used_bytes_of_the_containing_disk() {
        let temp = TempDir::new("scan-disk-total");
        temp.write_file("f.txt", b"1234567890");

        let root = temp.path().to_string_lossy().into_owned();

        let disks = vec![
            DiskInfo {
                name: "Test Disk".to_string(),
                mount_point: root.clone(),
                file_system: "apfs".to_string(),
                kind: "SSD".to_string(),
                is_removable: false,
                is_read_only: false,
                total_bytes: 1000,
                available_bytes: 500,
                used_bytes: 500,
                used_percent: 50.0,
            },
            DiskInfo {
                name: "Other Disk".to_string(),
                mount_point: "/some/other/mount".to_string(),
                file_system: "apfs".to_string(),
                kind: "SSD".to_string(),
                is_removable: false,
                is_read_only: false,
                total_bytes: 2000,
                available_bytes: 200,
                used_bytes: 1800,
                used_percent: 90.0,
            },
        ];

        let (channel, received) = collecting_channel();
        let scan_results = Arc::new(RwLock::new(ScanIndex::default()));

        scan_directory_internal(root, disks, channel, scan_results).unwrap();

        let messages = received.lock().unwrap();
        match messages.first() {
            Some(ScanMessage::Start { total_bytes, .. }) => assert_eq!(*total_bytes, 500),
            other => panic!("expected Start message first, got {other:?}"),
        }
    }

    #[test]
    fn returns_an_error_instead_of_panicking_when_the_scan_index_lock_is_poisoned() {
        let scan_results = Arc::new(RwLock::new(ScanIndex::default()));

        let poisoning_handle = {
            let scan_results = scan_results.clone();
            std::thread::spawn(move || {
                let _guard = scan_results.write().unwrap();
                panic!("deliberately poisoning the lock for this test");
            })
        };
        // The spawned thread's panic is expected; joining just waits for it.
        let _ = poisoning_handle.join();

        let temp = TempDir::new("scan-poisoned-lock");
        let (channel, _) = collecting_channel();

        let result = scan_directory_internal(
            temp.path().to_string_lossy().into_owned(),
            vec![],
            channel,
            scan_results,
        );

        assert!(result.is_err());
    }
}