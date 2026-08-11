use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub size_bytes: u64,
}

/// Sums file sizes under `path`, recursing only within `device` so that other
/// mounted volumes reachable from within (e.g. /Volumes/*, firmlinked data
/// volumes) aren't double-counted into this folder's total.
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

fn dir_total_size(path: &Path) -> u64 {
    match fs::metadata(path) {
        Ok(metadata) => dir_size_on_device(path, metadata.dev()),
        Err(_) => 0,
    }
}

#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FsEntry>, String> {
    let dir = Path::new(&path);
    let read_dir = fs::read_dir(dir).map_err(|e| e.to_string())?;
    let items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();

    let mut entries: Vec<FsEntry> = items
        .into_par_iter()
        .filter_map(|item| {
            let file_type = item.file_type().ok()?;
            let is_dir = file_type.is_dir();
            let is_symlink = file_type.is_symlink();
            let entry_path = item.path();

            let size_bytes = if is_dir {
                dir_total_size(&entry_path)
            } else {
                item.metadata().map(|m| m.len()).unwrap_or(0)
            };

            Some(FsEntry {
                name: item.file_name().to_string_lossy().to_string(),
                path: entry_path.to_string_lossy().to_string(),
                is_dir,
                is_symlink,
                size_bytes,
            })
        })
        .collect();

    entries.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use std::time::{Duration, Instant};

    #[test]
    fn dir_total_size_terminates_on_symlink_cycle_back_to_ancestor() {
        let tmp = std::env::temp_dir().join(format!(
            "disk-inventory-test-{}",
            std::process::id()
        ));
        let child = tmp.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("a.txt"), b"hello").unwrap();
        // Mimic /Volumes/Macintosh HD -> / : a symlink inside `child`
        // that points back up at `tmp`, its own ancestor.
        symlink(&tmp, child.join("loop")).unwrap();

        let start = Instant::now();
        let size = dir_total_size(&tmp);
        let elapsed = start.elapsed();

        fs::remove_dir_all(&tmp).unwrap();

        assert!(
            elapsed < Duration::from_secs(5),
            "dir_total_size took too long, likely an infinite loop: {elapsed:?}"
        );
        assert_eq!(size, 5, "expected only a.txt's 5 bytes, got {size}");
    }

    #[test]
    fn list_directory_shows_recursive_folder_size_and_sorts_descending() {
        let tmp = std::env::temp_dir().join(format!(
            "disk-inventory-test-sort-{}",
            std::process::id()
        ));
        fs::create_dir_all(&tmp).unwrap();

        // small_folder/: two files totalling 30 bytes.
        let small_folder = tmp.join("small_folder");
        fs::create_dir_all(&small_folder).unwrap();
        fs::write(small_folder.join("x.txt"), vec![b'x'; 10]).unwrap();
        fs::write(small_folder.join("y.txt"), vec![b'y'; 20]).unwrap();

        // big_folder/: one file, 1000 bytes — should outrank small_folder
        // and big_file.bin even though it's a directory, not a file.
        let big_folder = tmp.join("big_folder");
        fs::create_dir_all(&big_folder).unwrap();
        fs::write(big_folder.join("z.bin"), vec![0u8; 1000]).unwrap();

        // A lone file, bigger than small_folder but smaller than big_folder.
        fs::write(tmp.join("mid_file.bin"), vec![0u8; 100]).unwrap();

        let entries = list_directory(tmp.to_string_lossy().to_string()).unwrap();

        fs::remove_dir_all(&tmp).unwrap();

        let names_and_sizes: Vec<(String, u64)> = entries
            .iter()
            .map(|e| (e.name.clone(), e.size_bytes))
            .collect();

        assert_eq!(
            names_and_sizes,
            vec![
                ("big_folder".to_string(), 1000),
                ("mid_file.bin".to_string(), 100),
                ("small_folder".to_string(), 30),
            ],
            "expected entries sorted by total size descending, folders included"
        );
    }
}
