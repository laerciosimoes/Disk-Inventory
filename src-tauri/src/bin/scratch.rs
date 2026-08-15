use disk_inventory_lib::filesystem::{AppState, ScanMessage, scan_directory_internal};
use std::io::{self, Write};
use std::sync::Mutex;
use std::time::Instant;
use num_format::{Locale, ToFormattedString};
use chrono::Local;
struct ScanProgress {
    current_file: String,
    percent: u64,
    files_scanned: u64,
    scanned_bytes: u64,
    total_bytes: u64,
    started: bool,
}

fn tail_chars(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(n);
    chars[start..].iter().collect()
}

fn redraw(state: &mut ScanProgress) {
    if state.started {
        print!("\x1B[3A"); // Move cursor up 3 lines
    }
    state.started = true;

    let percent = state.percent.min(100);
    let width = 40u64;
    let filled = (percent * width) / 100;
    let bar = "#".repeat(filled as usize) + &"-".repeat((width - filled) as usize);

    let filename = std::path::Path::new(&state.current_file)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| state.current_file.clone());

    // Truncate to 80 chars to prevent terminal wrapping from breaking ANSI movement
    print!("\r\x1B[K{}\n", tail_chars(&state.current_file, 80));
    print!("\r\x1B[K{}\n", tail_chars(&filename, 80));
    print!(
        "\r\x1B[K[{bar}] {percent:>3}%  ({} files scanned)\n",
        state.files_scanned.to_formatted_string(&Locale::pt)
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
        current_file: String::new(),
        percent: 0,
        files_scanned: 0,
        scanned_bytes: 0,
        total_bytes: 0,
        started: false,
    });

    let channel: tauri::ipc::Channel<ScanMessage> =
        tauri::ipc::Channel::new(move |message| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = message {
                if let Ok(msg) = serde_json::from_str::<ScanMessage>(&json) {
                    match msg {
                        ScanMessage::Start { total_bytes } => {
                            let mut state = progress.lock().unwrap();
                            state.total_bytes = total_bytes;
                        }

                        ScanMessage::Entries(entries) => {
                            let mut state = progress.lock().unwrap();
                            if let Some(last) = entries.last() {
                                state.current_file = last.path.clone();
                            }
                        }

                        ScanMessage::Progress { scanned_files, scanned_bytes } => {
                            let mut state = progress.lock().unwrap();
                            state.files_scanned = scanned_files;
                            state.scanned_bytes = scanned_bytes;
                            

                            if state.total_bytes > 0 {
                                state.percent = (scanned_bytes * 100) / state.total_bytes;
                            }

                            redraw(&mut state);
                        }

                        ScanMessage::Complete => {
                            let mut state = progress.lock().unwrap();
                            state.percent = 100;
                            redraw(&mut state);
                            println!("\nCOMPLETE");
                        }
                    }
                }
            }
            Ok(())
        });

    let disks = disk_inventory_lib::disks::list_disks();

    match scan_directory_internal(path.clone(), disks, channel, &app_state) {
        Ok(_) => {
            println!("scan_directory finished successfully.");
            // Inspect top-level directory items from memory
            //let results = app_state.scan_results.lock().unwrap();
            //if let Some(items) = results.get(&path) {
            //    println!("\nTop items in '{path}':");
            //    for item in items.iter().take(5) {
            //        println!("  {} - {} bytes", item.path, item.size_bytes);
            //    }
            //}
        }
        Err(e) => eprintln!("Error during scan: {e}"),
    }

    let end_time = Local::now();
    let elapsed = start_instant.elapsed();
    println!("End time: {}", end_time.format("%Y-%m-%d %H:%M:%S%.3f"));
    println!("Elapsed: {:.3}s", elapsed.as_secs_f64());
}