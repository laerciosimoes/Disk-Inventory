pub mod disks;
pub mod filesystem;
pub mod windows;

use disks::list_disks;
use filesystem::scan_directory;
use tauri::{Manager, WindowEvent};
use windows::{close_current_window, get_window_mount_point, open_volume_window, VolumeWindowState};

use crate::filesystem::get_directory_contents;

/// macOS QoS class constants and the pthread call to apply one to the
/// current thread, declared directly against libSystem rather than pulled
/// in as a dependency (this app already has a small dependency footprint).
/// See `<pthread/qos.h>`.
#[cfg(target_os = "macos")]
mod qos {
    use std::os::raw::c_int;

    pub const QOS_CLASS_BACKGROUND: u32 = 0x09;

    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: c_int) -> c_int;
    }

    /// Marks the current thread as background-priority so macOS's scheduler
    /// always favors interactive work (the webview's render/compositor
    /// thread) over it, regardless of how many logical cores are free.
    pub fn mark_current_thread_background() {
        unsafe {
            pthread_set_qos_class_self_np(QOS_CLASS_BACKGROUND, 0);
        }
    }
}

/// Cap rayon's global pool below the core count and run its workers at
/// background QoS, so a large recursive size scan (see
/// `filesystem::dir_size_on_device`) can't starve the webview's own
/// render/compositor thread — which would otherwise leave the window blank,
/// including the loading indicator itself, for the whole scan. QoS is the
/// part that actually matters: under contention (e.g. a virtualized host
/// oversubscribing cores) even a couple of default-priority threads can
/// still visibly starve the UI, where background-QoS threads reliably
/// yield to it.
///
/// The workload is metadata syscalls (`stat`), not CPU compute: past a
/// modest thread count, more parallelism mostly adds VFS/kernel lock
/// contention rather than throughput, so a small fixed cap is also a
/// performance win on its own.
fn init_bounded_rayon_pool() {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let worker_threads = (cores / 4).clamp(2, 4);

    let builder = rayon::ThreadPoolBuilder::new().num_threads(worker_threads);

    #[cfg(target_os = "macos")]
    let builder = builder.start_handler(|_| qos::mark_current_thread_background());

    let _ = builder.build_global();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_bounded_rayon_pool();

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
            get_directory_contents,
            scan_directory,
            open_volume_window,
            get_window_mount_point,
            close_current_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
