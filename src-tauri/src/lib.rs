pub mod disks;
pub mod filesystem;
pub mod windows;

use disks::list_disks;
use filesystem::scan_directory;
use tauri::{Manager, WindowEvent};
use windows::{forget_window, get_window_mount_point, open_volume_window, VolumeWindowState};

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

/// The number of rayon worker threads to run for a given core count: a
/// quarter of the cores, clamped to a small [2, 4] range regardless of how
/// large the machine is.
fn worker_thread_count(cores: usize) -> usize {
    (cores / 4).clamp(2, 4)
}

/// Builds (but does not install) the bounded, background-QoS rayon pool
/// configuration.
///
/// Split out of `init_bounded_rayon_pool` so tests can build a local pool
/// from the exact same configuration instead of racing on the process-wide
/// global pool, which can only be installed once — `jwalk` (used by
/// `filesystem::scan_directory_internal`) also depends on rayon, so a test
/// installing the global pool can race against a concurrently-running scan
/// test that lazily triggers rayon's own (uncapped) default global pool
/// first.
fn bounded_rayon_pool_builder() -> rayon::ThreadPoolBuilder {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let worker_threads = worker_thread_count(cores);

    let builder = rayon::ThreadPoolBuilder::new().num_threads(worker_threads);

    #[cfg(target_os = "macos")]
    let builder = builder.start_handler(|_| qos::mark_current_thread_background());

    builder
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
    let _ = bounded_rayon_pool_builder().build_global();
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
                    forget_window(window.label(), &state);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_disks,
            get_directory_contents,
            scan_directory,
            open_volume_window,
            get_window_mount_point
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// -----------------------------------------------------------------------------
// Tests
//
// `run()` itself is not exercised here: it starts the real Tauri event loop
// and would open an actual GUI window on the developer's machine. Everything
// it delegates to (window cleanup, thread pool sizing) is tested directly
// below instead.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_thread_count_is_clamped_between_2_and_4() {
        assert_eq!(worker_thread_count(1), 2);
        assert_eq!(worker_thread_count(4), 2);
        assert_eq!(worker_thread_count(8), 2);
        assert_eq!(worker_thread_count(12), 3);
        assert_eq!(worker_thread_count(16), 4);
        assert_eq!(worker_thread_count(64), 4);
    }

    #[test]
    fn caps_the_rayon_pool_within_the_expected_bounds() {
        // A local (non-global) pool, so this can't race against other
        // tests that transitively touch rayon's process-wide global pool
        // (see `bounded_rayon_pool_builder`'s doc comment).
        let pool = bounded_rayon_pool_builder().build().unwrap();

        assert!((2..=4).contains(&pool.current_num_threads()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn marking_the_current_thread_background_does_not_panic() {
        qos::mark_current_thread_background();
    }
}
