use disk_inventory_lib::filesystem::{AppState, ScanMessage, scan_directory_internal};
use std::io::{self, Write};
use std::sync::Mutex;
use std::time::Instant;
use num_format::{Locale, ToFormattedString};
use chrono::Local;

struct ScanProgress {
    percent: u64,
    files_scanned: u64,
    scanned_bytes: u64,
    total_bytes: u64,
    generation: u64,
    started: bool,
}

fn redraw(state: &mut ScanProgress) {
    if state.started {
        print!("\x1B[2A"); // Move cursor up 2 lines
    }
    state.started = true;

    let percent = state.percent.min(100);
    let width = 40u64;
    let filled = (percent * width) / 100;
    let bar = "#".repeat(filled as usize) + &"-".repeat((width - filled) as usize);

    print!(
        "\r\x1B[K[{bar}] {percent:>3}%  (generation {})\n",
        state.generation
    );
    print!(
        "\r\x1B[K{} files scanned, {} bytes\n",
        state.files_scanned.to_formatted_string(&Locale::pt),
        state.scanned_bytes.to_formatted_string(&Locale::pt)
    );
    let _ = io::stdout().flush();
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());

    println!("== scan_directory({path:?}) ==");
    let app_state = AppState::default();

    let start_instant = Instant::now();
    let start_time = Local::now();
    println!("Start time: {}", start_time.format("%Y-%m-%d %H:%M:%S%.3f"));

    let progress = Mutex::new(ScanProgress {
        percent: 0,
        files_scanned: 0,
        scanned_bytes: 0,
        total_bytes: 0,
        generation: 0,
        started: false,
    });

    let channel: tauri::ipc::Channel<ScanMessage> =
        tauri::ipc::Channel::new(move |message| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = message {
                if let Ok(msg) = serde_json::from_str::<ScanMessage>(&json) {
                    match msg {
                        ScanMessage::Start {
                            total_bytes,
                            generation,
                        } => {
                            let mut state = progress.lock().unwrap();
                            state.total_bytes = total_bytes;
                            state.generation = generation;
                        }

                        ScanMessage::Progress {
                            scanned_files,
                            scanned_bytes,
                            generation,
                        } => {
                            let mut state = progress.lock().unwrap();
                            state.files_scanned = scanned_files;
                            state.scanned_bytes = scanned_bytes;
                            state.generation = generation;

                            if state.total_bytes > 0 {
                                state.percent = (scanned_bytes * 100) / state.total_bytes;
                            }

                            redraw(&mut state);
                        }

                        ScanMessage::Complete { generation } => {
                            let mut state = progress.lock().unwrap();
                            state.percent = 100;
                            state.generation = generation;
                            redraw(&mut state);
                            println!("\nCOMPLETE");
                        }
                    }
                }
            }
            Ok(())
        });

    let disks = disk_inventory_lib::disks::list_disks();

    match scan_directory_internal(path.clone(), disks, channel, app_state.scan_results.clone()) {
        Ok(_) => {
            println!("scan_directory finished successfully.");
        }
        Err(e) => eprintln!("Error during scan: {e}"),
    }

    let end_time = Local::now();
    let elapsed = start_instant.elapsed();
    println!("End time: {}", end_time.format("%Y-%m-%d %H:%M:%S%.3f"));
    println!("Elapsed: {:.3}s", elapsed.as_secs_f64());
}
