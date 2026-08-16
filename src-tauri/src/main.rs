// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    disk_inventory_lib::run()
}

// -----------------------------------------------------------------------------
// Tests
//
// main.rs's only responsibility is handing off to `disk_inventory_lib::run()`.
// We deliberately never call it here: `run()` starts the real Tauri event
// loop and opens an actual GUI window on the developer's machine, which is
// exactly the pixel/GUI-driving this repo's testing guidance says to avoid.
// Instead, this locks in that `run` still exists as a plain, callable,
// no-argument entry point — a signature change or its removal fails to
// compile rather than silently breaking `main`.
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #[test]
    fn run_entry_point_has_expected_signature() {
        let _entry_point: fn() = disk_inventory_lib::run;
    }
}
