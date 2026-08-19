use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::url::{CFURL, CFURLRef};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;

/// `kLSRolesAll` from `<CoreServices/LaunchServices/LSInfo.h>`: match an
/// application regardless of the role (viewer/editor/shell/...) it's
/// registered for.
const LS_ROLES_ALL: u32 = 0xFFFF_FFFF;

// `LSCopyApplicationURLsForURL`/`LSCopyDefaultApplicationURLForURL` live in
// the LaunchServices sub-framework, but are re-exported through the
// CoreServices umbrella framework, so that's what gets linked.
#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSCopyApplicationURLsForURL(
        in_url: CFURLRef,
        in_role_mask: u32,
    ) -> core_foundation::array::CFArrayRef;

    fn LSCopyDefaultApplicationURLForURL(
        in_url: CFURLRef,
        in_role_mask: u32,
        out_error: *mut core_foundation::error::CFErrorRef,
    ) -> CFURLRef;
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub path: String,
    pub is_default: bool,
}

fn app_display_name(app_path: &Path) -> String {
    app_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| app_path.to_string_lossy().into_owned())
}

/// Every application macOS's LaunchServices database has registered as able
/// to open `path`, most-relevant (the current default) first.
///
/// Uses the same LaunchServices machinery Finder's own "Open With" submenu
/// is built from, rather than a hardcoded app list, so it stays correct as
/// the user installs/uninstalls apps.
#[tauri::command]
pub fn list_apps_for_path(path: String) -> Result<Vec<AppInfo>, String> {
    let url = CFURL::from_path(&path, false)
        .ok_or_else(|| format!("Could not build a file URL for '{path}'"))?;

    let default_path: Option<PathBuf> = unsafe {
        let default_ref =
            LSCopyDefaultApplicationURLForURL(url.as_concrete_TypeRef(), LS_ROLES_ALL, ptr::null_mut());

        if default_ref.is_null() {
            None
        } else {
            let default_url: CFURL = TCFType::wrap_under_create_rule(default_ref);
            default_url.to_path()
        }
    };

    let apps_ref = unsafe { LSCopyApplicationURLsForURL(url.as_concrete_TypeRef(), LS_ROLES_ALL) };

    if apps_ref.is_null() {
        return Ok(Vec::new());
    }

    let apps: CFArray<CFURL> = unsafe { TCFType::wrap_under_create_rule(apps_ref) };

    let mut result: Vec<AppInfo> = apps
        .iter()
        .filter_map(|item| {
            let app_path = item.to_path()?;

            Some(AppInfo {
                is_default: default_path.as_deref() == Some(app_path.as_path()),
                name: app_display_name(&app_path),
                path: app_path.to_string_lossy().into_owned(),
            })
        })
        .collect();

    result.sort_by(|a, b| b.is_default.cmp(&a.is_default).then_with(|| a.name.cmp(&b.name)));

    Ok(result)
}

fn run_open(args: &[&str]) -> Result<(), String> {
    let status = Command::new("open")
        .args(args)
        .status()
        .map_err(|error| format!("Failed to launch 'open': {error}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("'open' exited with status {status}"))
    }
}

#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    run_open(&[&path])
}

#[tauri::command]
pub fn open_path_with(path: String, app_path: String) -> Result<(), String> {
    run_open(&["-a", &app_path, &path])
}

#[tauri::command]
pub fn reveal_in_finder(path: String) -> Result<(), String> {
    run_open(&["-R", &path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_display_name_strips_the_dot_app_extension() {
        assert_eq!(
            app_display_name(Path::new("/Applications/Preview.app")),
            "Preview"
        );
    }

    #[test]
    fn app_display_name_falls_back_to_the_full_path_without_a_file_name() {
        assert_eq!(app_display_name(Path::new("/")), "/");
    }
}
