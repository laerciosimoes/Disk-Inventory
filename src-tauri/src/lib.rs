mod disks;
mod filesystem;
mod windows;

use disks::list_disks;
use filesystem::list_directory;
use tauri::{Manager, WindowEvent};
use windows::{close_current_window, get_window_mount_point, open_volume_window, VolumeWindowState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(VolumeWindowState::default())
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                if let Some(state) = window.try_state::<VolumeWindowState>() {
                    if let Ok(mut map) = state.0.lock() {
                        map.remove(window.label());
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_disks,
            list_directory,
            open_volume_window,
            get_window_mount_point,
            close_current_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
