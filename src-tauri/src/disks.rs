use serde::{Deserialize, Serialize};
use sysinfo::Disks;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub kind: String,
    pub is_removable: bool,
    pub is_read_only: bool,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub used_percent: f64,
}

#[tauri::command]
pub fn list_disks() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();

    disks
        .iter()
        .filter(|disk| {
            let mount_path = disk.mount_point().to_string_lossy();

            // Ignore macOS APFS internal system volume mounts
            if cfg!(target_os = "macos") && mount_path.starts_with("/System/Volumes/") {
                return false;
            }

            true
        })
        .map(|disk| {
            let total = disk.total_space();
            let available = disk.available_space();
            let used = total.saturating_sub(available);
            let used_percent = if total > 0 {
                (used as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            let name = disk.name().to_string_lossy().to_string();
            let mount_point = disk.mount_point().to_string_lossy().to_string();

            DiskInfo {
                name: if name.is_empty() {
                    mount_point.clone()
                } else {
                    name
                },
                mount_point,
                file_system: disk.file_system().to_string_lossy().to_string(),
                kind: disk.kind().to_string(),
                is_removable: disk.is_removable(),
                is_read_only: disk.is_read_only(),
                total_bytes: total,
                available_bytes: available,
                used_bytes: used,
                used_percent,
            }
        })
        .collect()
}
