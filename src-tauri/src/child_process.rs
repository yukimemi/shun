//! Keep the AppImage runtime available to shun, but not to external programs.
//!
//! There are no reliable backups of AppRun's overwritten values. Remove only
//! attributable bundle paths/identity, rather than claiming to restore the
//! original environment (or discarding unrelated user configuration).

use std::ffi::OsStr;
use std::process::Command;

pub(crate) fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    sanitize(&mut command);
    command
}

pub(crate) fn sanitize(command: &mut Command) {
    #[cfg(target_os = "linux")]
    linux::sanitize(command);
    #[cfg(not(target_os = "linux"))]
    let _ = command;
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Component, Path};
    use std::process::Command;
    use std::sync::LazyLock;

    const PATH_LISTS: &[&str] = &[
        "PATH",
        "LD_LIBRARY_PATH",
        "PYTHONPATH",
        "PERLLIB",
        "XDG_DATA_DIRS",
        "GSETTINGS_SCHEMA_DIR",
        "QT_PLUGIN_PATH",
        "GST_PLUGIN_SYSTEM_PATH",
        "GST_PLUGIN_SYSTEM_PATH_1_0",
        "GTK_PATH",
        "GI_TYPELIB_PATH",
        // Python supports prefix:exec_prefix on Unix.
        "PYTHONHOME",
    ];
    const SINGLE_PATHS: &[&str] = &[
        "OWD",
        "GTK_DATA_PREFIX",
        "GTK_EXE_PREFIX",
        "GTK_IM_MODULE_FILE",
        "GDK_PIXBUF_MODULE_FILE",
        "GIO_MODULE_DIR",
    ];

    type Overrides = Vec<(&'static str, Option<OsString>)>;

    pub(super) fn sanitize(command: &mut Command) {
        // Parse inherited runtime paths once, without changing the process-wide
        // environment that GTK/WebKit still need on other threads.
        static OVERRIDES: LazyLock<Overrides> =
            LazyLock::new(|| overrides(|name| std::env::var_os(name)));
        apply_overrides(command, &OVERRIDES);
    }

    fn apply_overrides(command: &mut Command, overrides: &Overrides) {
        for (name, value) in overrides {
            match value {
                Some(value) => {
                    command.env(name, value);
                }
                None => {
                    command.env_remove(name);
                }
            }
        }
    }

    fn overrides(mut get: impl FnMut(&str) -> Option<OsString>) -> Overrides {
        let appdir_value = get("APPDIR");
        let appdir = appdir_value
            .as_deref()
            .map(Path::new)
            // A malformed APPDIR=/ must never classify all system paths as bundled.
            .filter(|path| path.is_absolute() && path.parent().is_some());
        let appimage = get("APPIMAGE").filter(|value| !value.is_empty());
        if appdir.is_none() && appimage.is_none() {
            return Vec::new();
        }

        let mut result = Vec::new();
        for &name in PATH_LISTS {
            if let Some(value) = get(name) {
                let filtered = filter_paths(&value, appdir);
                if filtered.as_deref() != Some(value.as_os_str()) {
                    result.push((name, filtered));
                }
            }
        }
        for &name in SINGLE_PATHS {
            if let Some(value) = get(name) {
                if is_bundle_path(&value, appdir) {
                    result.push((name, None));
                }
            }
        }

        if appdir_value.is_some() {
            result.push(("APPDIR", None));
        }
        if appimage.is_some() {
            result.push(("APPIMAGE", None));
        }
        if let Some(argv0) = get("ARGV0") {
            let matches_image = appimage.as_deref().is_some_and(|image| {
                argv0 == image
                    || Path::new(image)
                        .file_name()
                        .is_some_and(|name| argv0 == name)
                    || matches_relative_image(&argv0, image, get("OWD").as_deref())
            });
            if matches_image || is_bundle_path(&argv0, appdir) {
                result.push(("ARGV0", None));
            }
        }
        // GTK_THEME is not a path, and AppRun leaves no original-value backup.
        // Its value alone cannot distinguish a runtime override from user intent.
        result
    }

    fn matches_relative_image(argv0: &OsStr, image: &OsStr, owd: Option<&OsStr>) -> bool {
        let invocation = Path::new(argv0);
        let image = Path::new(image);
        if invocation.is_absolute() || !image.is_absolute() {
            return false;
        }
        // AppRun may change cwd before starting shun. OWD records the directory
        // in which the user invoked the image; never resolve against the bundle.
        let directory = owd
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::current_dir().ok());
        let Some(directory) = directory.filter(|path| path.is_absolute()) else {
            return false;
        };
        let resolved = directory.join(invocation);
        resolved == image
            || resolved.canonicalize().ok().is_some_and(|resolved| {
                image
                    .canonicalize()
                    .ok()
                    .is_some_and(|image| resolved == image)
            })
    }

    fn is_bundle_path(value: &OsStr, appdir: Option<&Path>) -> bool {
        let path = Path::new(value);
        path.is_absolute()
            && (appdir.is_some_and(|root| path.starts_with(root))
                || path.components().any(|component| {
                    matches!(component, Component::Normal(name)
                        if name.as_bytes().starts_with(b".mount_shun"))
                }))
    }

    fn filter_paths(value: &OsStr, appdir: Option<&Path>) -> Option<OsString> {
        let mut filtered = Vec::with_capacity(value.len());
        let mut removed = false;
        let mut has_entry = false;
        let mut has_path = false;
        for entry in value.as_bytes().split(|byte| *byte == b':') {
            if is_bundle_path(OsStr::from_bytes(entry), appdir) {
                removed = true;
                continue;
            }
            if has_entry {
                filtered.push(b':');
            }
            has_entry = true;
            has_path |= !entry.is_empty();
            filtered.extend_from_slice(entry);
        }
        // AppRun appends an empty fallback when no original value existed.
        // Do not turn bundle-only paths into a current-directory library search.
        // Otherwise preserve user-supplied empty path components verbatim.
        if removed && !has_path {
            None
        } else {
            Some(OsString::from_vec(filtered))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::collections::BTreeMap;

        fn environment(entries: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
            entries
                .iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value)))
                .collect()
        }

        // Read the actual exec'd child's environment, including non-UTF8 bytes.
        // Every input is per-Command; tests never mutate the test process env.
        fn child_environment(input: &BTreeMap<OsString, OsString>) -> BTreeMap<OsString, OsString> {
            let changes = overrides(|name| input.get(OsStr::new(name)).cloned());
            let mut child = Command::new("/usr/bin/env");
            child.arg("-0").env_clear().envs(input);
            apply_overrides(&mut child, &changes);
            let output = child.output().expect("execute system env");
            assert!(output.status.success(), "{:?}", output);
            output
                .stdout
                .split(|byte| *byte == 0)
                .filter(|entry| !entry.is_empty())
                .map(|entry| {
                    let separator = entry.iter().position(|byte| *byte == b'=').unwrap();
                    (
                        OsString::from_vec(entry[..separator].to_vec()),
                        OsString::from_vec(entry[separator + 1..].to_vec()),
                    )
                })
                .collect()
        }

        fn value<'a>(env: &'a BTreeMap<OsString, OsString>, name: &str) -> Option<&'a OsStr> {
            env.get(OsStr::new(name)).map(OsString::as_os_str)
        }

        #[test]
        fn removes_bundled_libraries_plugins_and_interpreter_paths_in_real_child() {
            let input = environment(&[
                ("APPDIR", "/tmp/.mount_shunCURRENT"),
                ("APPIMAGE", "/home/user/bin/shun"),
                ("ARGV0", "/home/user/bin/shun"),
                (
                    "LD_LIBRARY_PATH",
                    "/tmp/.mount_shunCURRENT/usr/lib:/tmp/.mount_shunOLD/lib:/user/lib",
                ),
                ("PYTHONHOME", "/tmp/.mount_shunCURRENT/usr"),
                (
                    "QT_PLUGIN_PATH",
                    "/tmp/.mount_shunCURRENT/usr/lib/qt5/plugins:/user/qt",
                ),
                (
                    "GSETTINGS_SCHEMA_DIR",
                    "/tmp/.mount_shunCURRENT/usr/share/glib-2.0/schemas:/user/schemas",
                ),
                (
                    "GTK_IM_MODULE_FILE",
                    "/tmp/.mount_shunCURRENT/usr/lib/gtk/immodules.cache",
                ),
                ("GIO_MODULE_DIR", "/tmp/.mount_shunOLD/usr/lib/gio/modules"),
                (
                    "XDG_DATA_DIRS",
                    "/tmp/.mount_shunCURRENT/usr/share:/usr/share",
                ),
            ]);
            let observed = child_environment(&input);
            assert_eq!(
                value(&observed, "LD_LIBRARY_PATH"),
                Some(OsStr::new("/user/lib"))
            );
            assert_eq!(
                value(&observed, "QT_PLUGIN_PATH"),
                Some(OsStr::new("/user/qt"))
            );
            assert_eq!(
                value(&observed, "GSETTINGS_SCHEMA_DIR"),
                Some(OsStr::new("/user/schemas"))
            );
            assert_eq!(
                value(&observed, "XDG_DATA_DIRS"),
                Some(OsStr::new("/usr/share"))
            );
            for name in [
                "APPDIR",
                "APPIMAGE",
                "ARGV0",
                "PYTHONHOME",
                "GTK_IM_MODULE_FILE",
                "GIO_MODULE_DIR",
            ] {
                assert_eq!(value(&observed, name), None, "{name}");
            }
        }

        #[test]
        fn preserves_user_paths_and_desktop_authentication_environment() {
            let input = environment(&[
                ("APPDIR", "/opt/shun.extracted"),
                ("APPIMAGE", "/home/user/bin/shun.AppImage"),
                ("ARGV0", "a-user-defined-value"),
                ("GTK_THEME", "MyTheme:dark"),
                ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
                ("WAYLAND_DISPLAY", "wayland-1"),
                ("DISPLAY", ":0"),
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("SSH_AUTH_SOCK", "/run/user/1000/ssh-agent"),
                ("HOME", "/home/user"),
                ("UNRELATED", "a::b"),
                ("PATH", "/user/bin::/usr/bin:"),
                ("LD_LIBRARY_PATH", ":/user/lib:"),
                ("GIO_MODULE_DIR", ""),
            ]);
            let mut expected = input.clone();
            expected.remove(OsStr::new("APPDIR"));
            expected.remove(OsStr::new("APPIMAGE"));
            assert_eq!(child_environment(&input), expected);
        }

        #[test]
        fn removes_stale_and_multiple_mounts_without_retaining_cwd_search() {
            let input = environment(&[
                ("APPDIR", "/tmp/.mount_shunCURRENT"),
                (
                    "LD_LIBRARY_PATH",
                    ":/tmp/.mount_shunOLD/usr/lib::/tmp/.mount_shunOTHER/usr/lib:",
                ),
                ("PYTHONPATH", "/tmp/.mount_shunOLD/python::"),
                ("PERLLIB", "/tmp/.mount_shunOLD/perl:"),
                (
                    "PATH",
                    "/tmp/.mount_shunCURRENT/usr/bin:/usr/bin:/tmp/.mount_shunOLD/usr/bin",
                ),
                ("OWD", "/tmp/.mount_shunOLD/work"),
            ]);
            let observed = child_environment(&input);
            for name in ["LD_LIBRARY_PATH", "PYTHONPATH", "PERLLIB", "OWD"] {
                assert_eq!(value(&observed, name), None, "{name}");
            }
            assert_eq!(value(&observed, "PATH"), Some(OsStr::new("/usr/bin")));
        }

        #[test]
        fn supports_extracted_appdir_and_respects_path_component_boundaries() {
            let input = environment(&[
                ("APPDIR", "/opt/shun.extracted/"),
                ("ARGV0", "/opt/shun.extracted/AppRun"),
                ("PATH", "/opt/shun.extracted/usr/bin:/opt/shun.extracted-old/bin:/opt/shun.extractedness/bin:/tmp/not.mount_shunOLD/bin:/tmp/.mount_other/bin:/usr/bin"),
                ("OWD", "/opt/shun.extracted-backup"),
                ("LD_LIBRARY_PATH", "/opt/shun.extracted"),
            ]);
            let observed = child_environment(&input);
            assert_eq!(
                value(&observed, "PATH"),
                Some(OsStr::new("/opt/shun.extracted-old/bin:/opt/shun.extractedness/bin:/tmp/not.mount_shunOLD/bin:/tmp/.mount_other/bin:/usr/bin"))
            );
            assert_eq!(
                value(&observed, "OWD"),
                Some(OsStr::new("/opt/shun.extracted-backup"))
            );
            assert_eq!(value(&observed, "LD_LIBRARY_PATH"), None);
            assert_eq!(value(&observed, "APPDIR"), None);
            assert_eq!(value(&observed, "ARGV0"), None);
        }

        #[test]
        fn native_environment_is_unchanged_including_empty_and_mount_like_values() {
            for identity in [
                None,
                Some(("APPDIR", "")),
                Some(("APPDIR", "/")),
                Some(("APPDIR", "relative/path")),
                Some(("APPIMAGE", "")),
            ] {
                let mut input = environment(&[
                    ("PATH", ":/tmp/.mount_shunOLD/bin::/usr/bin:"),
                    ("LD_LIBRARY_PATH", ""),
                    ("PYTHONHOME", "/user/python"),
                    ("ARGV0", "user-argv0"),
                ]);
                if let Some((name, contents)) = identity {
                    input.insert(name.into(), contents.into());
                }
                assert_eq!(child_environment(&input), input);
            }
        }

        #[test]
        fn appimage_identity_without_appdir_still_cleans_stale_mounts() {
            let input = environment(&[
                ("APPIMAGE", "/home/user/bin/shun.AppImage"),
                ("ARGV0", "shun.AppImage"),
                ("LD_LIBRARY_PATH", "/tmp/.mount_shunOLD/usr/lib:/user/lib"),
                ("PYTHONHOME", "/tmp/.mount_shunOLD/usr:/user/python"),
            ]);
            let observed = child_environment(&input);
            assert_eq!(value(&observed, "APPIMAGE"), None);
            assert_eq!(value(&observed, "ARGV0"), None);
            assert_eq!(
                value(&observed, "LD_LIBRARY_PATH"),
                Some(OsStr::new("/user/lib"))
            );
            assert_eq!(
                value(&observed, "PYTHONHOME"),
                Some(OsStr::new("/user/python"))
            );
        }

        #[test]
        fn relative_image_invocation_and_near_miss_in_real_child() {
            let mut input = environment(&[
                ("APPIMAGE", "/opt/images/shun.AppImage"),
                ("OWD", "/opt/images"),
                ("ARGV0", "./shun.AppImage"),
            ]);
            assert_eq!(value(&child_environment(&input), "ARGV0"), None);
            input.insert("ARGV0".into(), "./shun.AppImage-backup".into());
            assert_eq!(
                value(&child_environment(&input), "ARGV0"),
                value(&input, "ARGV0")
            );
        }

        #[test]
        fn relative_image_identity_uses_original_directory_in_real_child() {
            let original_directory = tempfile::tempdir().unwrap();
            // This directory differs from the test process cwd, as after AppRun
            // changes cwd. Include non-UTF8 bytes in the invocation and APPIMAGE.
            let image_name = OsStr::from_bytes(b"shun-\xff.AppImage");
            let image = original_directory.path().join(image_name);
            std::fs::write(&image, b"").unwrap();
            std::fs::create_dir(original_directory.path().join("subdir")).unwrap();
            let mut input = environment(&[("APPDIR", "/tmp/.mount_shunCURRENT")]);
            input.insert("APPIMAGE".into(), image.into_os_string());
            input.insert("OWD".into(), original_directory.path().into());
            for (invocation, removed) in [
                (b"./shun-\xff.AppImage".as_slice(), true),
                (b"subdir/../shun-\xff.AppImage".as_slice(), true),
                (b"./shun-\xff.AppImage-backup".as_slice(), false),
                (b"subdir/shun-\xff.AppImage".as_slice(), false),
            ] {
                let invocation = OsStr::from_bytes(invocation);
                input.insert("ARGV0".into(), invocation.into());
                let observed = child_environment(&input);
                assert_eq!(
                    value(&observed, "ARGV0"),
                    if removed { None } else { Some(invocation) },
                    "{invocation:?}"
                );
                assert_eq!(value(&observed, "OWD"), value(&input, "OWD"));
                assert_eq!(value(&observed, "APPIMAGE"), None);
                assert_eq!(value(&observed, "APPDIR"), None);
                // The inherited input stays intact; only the child is sanitized.
                assert_eq!(value(&input, "ARGV0"), Some(invocation));
                assert!(value(&input, "APPIMAGE").is_some());
            }
        }

        #[test]
        fn root_appdir_does_not_remove_system_paths_or_other_appimage_mounts() {
            let input = environment(&[
                ("APPDIR", "/"),
                ("APPIMAGE", "/home/user/bin/shun"),
                ("PATH", "/usr/bin:/bin:/tmp/.mount_otherAPP/usr/bin"),
                ("LD_LIBRARY_PATH", "/user/lib:/tmp/.mount_shunOLD/lib:"),
            ]);
            let observed = child_environment(&input);
            assert_eq!(
                value(&observed, "PATH"),
                Some(OsStr::new("/usr/bin:/bin:/tmp/.mount_otherAPP/usr/bin"))
            );
            assert_eq!(
                value(&observed, "LD_LIBRARY_PATH"),
                Some(OsStr::new("/user/lib:"))
            );
            assert_eq!(value(&observed, "APPDIR"), None);
        }

        #[test]
        fn non_utf8_paths_and_values_survive_child_exec_without_loss() {
            let mut input = environment(&[("APPIMAGE", "/home/user/bin/shun")]);
            input.insert(
                "APPDIR".into(),
                OsString::from_vec(b"/opt/shun-\xff".to_vec()),
            );
            input.insert(
                "LD_LIBRARY_PATH".into(),
                OsString::from_vec(b"/opt/shun-\xff/lib:/user/\xfe/lib:/tmp/.mount_shun\xfd/lib:/opt/shun-\xff-other/lib".to_vec()),
            );
            input.insert("OWD".into(), OsString::from_vec(b"/user/\xfc".to_vec()));
            input.insert(
                "UNRELATED".into(),
                OsString::from_vec(b"keep-\xfb".to_vec()),
            );
            let observed = child_environment(&input);
            assert_eq!(
                value(&observed, "LD_LIBRARY_PATH"),
                Some(OsStr::from_bytes(
                    b"/user/\xfe/lib:/opt/shun-\xff-other/lib"
                ))
            );
            assert_eq!(
                value(&observed, "OWD"),
                Some(OsStr::from_bytes(b"/user/\xfc"))
            );
            assert_eq!(
                value(&observed, "UNRELATED"),
                Some(OsStr::from_bytes(b"keep-\xfb"))
            );
        }
    }
}
