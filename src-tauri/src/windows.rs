use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, State, WebviewUrl, WebviewWindowBuilder, Window};

pub struct VolumeWindowState(pub Mutex<HashMap<String, String>>);

impl Default for VolumeWindowState {
    fn default() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

static WINDOW_COUNTER: AtomicU64 = AtomicU64::new(0);

#[tauri::command]
pub fn open_volume_window(
    app: AppHandle,
    window: Window,
    state: State<VolumeWindowState>,
    mount_point: String,
) -> Result<(), String> {
    let id = WINDOW_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("volume-{id}");

    state
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .insert(label.clone(), mount_point.clone());

    WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title(format!("Disk Inventory — {mount_point}"))
        .inner_size(900.0, 900.0)
        .build()
        .map_err(|e| e.to_string())?;

    window.close().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn get_window_mount_point(window: Window, state: State<VolumeWindowState>) -> Option<String> {
    state.0.lock().ok()?.get(window.label()).cloned()
}

/// Forgets a destroyed window's mount point.
///
/// Called from `lib.rs`'s `on_window_event` hook when a volume window
/// closes, so `VolumeWindowState` doesn't accumulate stale labels for the
/// lifetime of the app. Split out as a plain function (taking the label
/// rather than a live `Window`) so it can be unit tested directly.
pub fn forget_window(label: &str, state: &VolumeWindowState) {
    if let Ok(mut map) = state.0.lock() {
        map.remove(label);
    }
}

// -----------------------------------------------------------------------------
// Tests
//
// The commands above take live `AppHandle`/`Window` values that require a
// running Tauri runtime to construct, so they aren't exercised directly here.
// These tests cover the state/logic those commands rely on: the
// `VolumeWindowState` map operations mirrored by `open_volume_window` /
// `get_window_mount_point` / the window-destroyed cleanup hook in `lib.rs`,
// and the `WINDOW_COUNTER`-based label generation used by
// `open_volume_window`.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_starts_empty() {
        let state = VolumeWindowState::default();
        assert!(state.0.lock().unwrap().is_empty());
    }

    #[test]
    fn tracks_mount_point_by_label() {
        let state = VolumeWindowState::default();

        state
            .0
            .lock()
            .unwrap()
            .insert("volume-42".to_string(), "/Volumes/Data".to_string());

        let map = state.0.lock().unwrap();
        assert_eq!(map.get("volume-42"), Some(&"/Volumes/Data".to_string()));
        assert_eq!(map.get("volume-missing"), None);
    }

    #[test]
    fn forget_window_removes_only_the_matching_label() {
        let state = VolumeWindowState::default();

        state
            .0
            .lock()
            .unwrap()
            .insert("volume-1".to_string(), "/Volumes/Backup".to_string());
        state
            .0
            .lock()
            .unwrap()
            .insert("volume-2".to_string(), "/Volumes/Data".to_string());

        forget_window("volume-1", &state);

        let map = state.0.lock().unwrap();
        assert_eq!(map.get("volume-1"), None);
        assert_eq!(map.get("volume-2"), Some(&"/Volumes/Data".to_string()));
    }

    #[test]
    fn forget_window_on_an_unknown_label_is_a_no_op() {
        let state = VolumeWindowState::default();

        state
            .0
            .lock()
            .unwrap()
            .insert("volume-1".to_string(), "/Volumes/Backup".to_string());

        forget_window("volume-missing", &state);

        assert_eq!(
            state.0.lock().unwrap().get("volume-1"),
            Some(&"/Volumes/Backup".to_string())
        );
    }

    #[test]
    fn window_counter_produces_unique_increasing_labels() {
        let first = WINDOW_COUNTER.fetch_add(1, Ordering::SeqCst);
        let second = WINDOW_COUNTER.fetch_add(1, Ordering::SeqCst);

        assert!(second > first);
        assert_ne!(format!("volume-{first}"), format!("volume-{second}"));
    }
}
