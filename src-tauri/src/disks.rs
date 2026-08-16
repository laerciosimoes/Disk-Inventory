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

/// Whether `mount_point` is one of macOS's internal APFS system-volume
/// mounts (e.g. `/System/Volumes/Data`), which duplicate the same physical
/// disk already surfaced under its real mount point and so should not be
/// listed separately.
fn is_internal_macos_volume(mount_point: &str) -> bool {
    cfg!(target_os = "macos") && mount_point.starts_with("/System/Volumes/")
}

/// Computes the derived `DiskInfo` fields (used bytes/percent, name
/// fallback) from the raw values `sysinfo` reports.
///
/// Split out of `list_disks` so this logic can be unit tested without
/// depending on whatever disks happen to be attached to the machine running
/// the tests.
fn build_disk_info(
    name: String,
    mount_point: String,
    file_system: String,
    kind: String,
    is_removable: bool,
    is_read_only: bool,
    total_bytes: u64,
    available_bytes: u64,
) -> DiskInfo {
    let used_bytes = total_bytes.saturating_sub(available_bytes);
    let used_percent = if total_bytes > 0 {
        (used_bytes as f64 / total_bytes as f64) * 100.0
    } else {
        0.0
    };

    DiskInfo {
        name: if name.is_empty() { mount_point.clone() } else { name },
        mount_point,
        file_system,
        kind,
        is_removable,
        is_read_only,
        total_bytes,
        available_bytes,
        used_bytes,
        used_percent,
    }
}

#[tauri::command]
pub fn list_disks() -> Vec<DiskInfo> {
    let disks = Disks::new_with_refreshed_list();

    disks
        .iter()
        .filter(|disk| !is_internal_macos_volume(&disk.mount_point().to_string_lossy()))
        .map(|disk| {
            build_disk_info(
                disk.name().to_string_lossy().to_string(),
                disk.mount_point().to_string_lossy().to_string(),
                disk.file_system().to_string_lossy().to_string(),
                disk.kind().to_string(),
                disk.is_removable(),
                disk.is_read_only(),
                disk.total_space(),
                disk.available_space(),
            )
        })
        .collect()
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // is_internal_macos_volume
    // -------------------------------------------------------------------------

    #[test]
    fn flags_system_volumes_mount_points() {
        assert!(is_internal_macos_volume("/System/Volumes/Data"));
        assert!(is_internal_macos_volume("/System/Volumes/VM"));
    }

    #[test]
    fn does_not_flag_regular_mount_points() {
        assert!(!is_internal_macos_volume("/"));
        assert!(!is_internal_macos_volume("/Volumes/Backup"));
    }

    // -------------------------------------------------------------------------
    // build_disk_info
    // -------------------------------------------------------------------------

    #[test]
    fn computes_used_bytes_and_percent() {
        let info = build_disk_info(
            "Macintosh HD".to_string(),
            "/".to_string(),
            "apfs".to_string(),
            "SSD".to_string(),
            false,
            false,
            1000,
            250,
        );

        assert_eq!(info.used_bytes, 750);
        assert_eq!(info.used_percent, 75.0);
    }

    #[test]
    fn falls_back_to_the_mount_point_when_the_disk_name_is_empty() {
        let info = build_disk_info(
            "".to_string(),
            "/Volumes/Backup".to_string(),
            "apfs".to_string(),
            "SSD".to_string(),
            false,
            false,
            1000,
            500,
        );

        assert_eq!(info.name, "/Volumes/Backup");
    }

    #[test]
    fn keeps_the_disk_name_when_present() {
        let info = build_disk_info(
            "Backup Drive".to_string(),
            "/Volumes/Backup".to_string(),
            "apfs".to_string(),
            "SSD".to_string(),
            false,
            false,
            1000,
            500,
        );

        assert_eq!(info.name, "Backup Drive");
    }

    #[test]
    fn used_percent_is_zero_for_a_zero_sized_disk() {
        let info = build_disk_info(
            "Empty".to_string(),
            "/mnt/empty".to_string(),
            "".to_string(),
            "".to_string(),
            false,
            false,
            0,
            0,
        );

        assert_eq!(info.used_bytes, 0);
        assert_eq!(info.used_percent, 0.0);
    }

    #[test]
    fn used_bytes_never_underflows_when_available_exceeds_total() {
        // sysinfo can (rarely) report available > total transiently; the
        // saturating subtraction must not panic or wrap.
        let info = build_disk_info(
            "Weird".to_string(),
            "/mnt/weird".to_string(),
            "".to_string(),
            "".to_string(),
            false,
            false,
            100,
            150,
        );

        assert_eq!(info.used_bytes, 0);
    }

    // -------------------------------------------------------------------------
    // list_disks
    // -------------------------------------------------------------------------

    #[test]
    fn list_disks_excludes_macos_system_volumes() {
        for disk in list_disks() {
            assert!(!is_internal_macos_volume(&disk.mount_point));
        }
    }
}
