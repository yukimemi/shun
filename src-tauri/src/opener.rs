//! OS associations with child-only AppImage environment cleanup on Linux.

use std::path::Path;

pub(crate) fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        linux::open(url.as_ref()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
    }
}

pub(crate) fn open_path(path: impl AsRef<Path>) -> Result<(), String> {
    let path = path.as_ref();
    #[cfg(target_os = "linux")]
    {
        // Keep tauri-plugin-opener's synchronous missing/inaccessible-path error.
        path.metadata().map_err(|e| e.to_string())?;
        linux::open(path.as_os_str()).map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        ffi::OsStr,
        io,
        os::unix::process::CommandExt,
        process::{Command, Stdio},
        thread,
    };

    fn commands(target: &OsStr) -> impl Iterator<Item = Command> {
        // Reuse the plugin's opener order (including wslview on WSL), passing
        // the URL/path as an argument rather than interpolating shell code.
        open::commands(target).into_iter().map(|mut command| {
            crate::child_process::sanitize(&mut command);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0);
            command
        })
    }

    pub(super) fn open(target: &OsStr) -> io::Result<()> {
        let mut last_error = None;
        for mut command in commands(target) {
            match command.spawn() {
                Ok(mut child) => {
                    // A separate process group and null stdio let the helper
                    // outlive shun without unsafe fork hooks in this multithreaded
                    // process. Reap it without waiting for the associated app.
                    thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return Ok(());
                }
                Err(error) => last_error = Some(error),
            }
        }
        // open::commands always provides at least xdg-open on Linux.
        Err(last_error.expect("no association opener commands"))
    }
}
