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

#[tauri::command]
pub fn close_current_window(window: Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}
