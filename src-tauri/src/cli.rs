//! Command line of a running/starting shun.
//!
//! Linux forwards to shun's public resident D-Bus API before initializing the GUI.
//! Other platforms retain the single-instance plugin transport. The plugin also
//! guards process ownership on Linux when concurrent cold starts race.
//!
//! ```text
//! shun                  show the launcher
//! shun show             show the launcher (also when starting fresh)
//! shun toggle           show / hide the launcher (the `launch` hotkey)
//! shun hotkey <KEY>     run the `[[apps]]` entry whose `hotkey` is <KEY> (e.g. "F10")
//! shun install-desktop  Linux: write the .desktop entry + icon, then exit
//! ```

/// D-Bus activation starts ownership only; the queued public method supplies the action.
#[cfg(target_os = "linux")]
pub(crate) const RESIDENT_ARG: &str = "--resident";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Show,
    Toggle,
    /// A hotkey as written in `config.toml`.
    Hotkey(String),
    InstallDesktop,
    /// Linux D-Bus activation bootstrap, never an application operation.
    #[cfg(target_os = "linux")]
    Resident,
}

/// Parses the arguments after the program name. `None` for no/unknown arguments.
pub fn parse<S: AsRef<str>>(args: &[S]) -> Option<Command> {
    match args {
        [c] if c.as_ref() == "show" => Some(Command::Show),
        [c] if c.as_ref() == "toggle" => Some(Command::Toggle),
        [c] if c.as_ref() == "install-desktop" => Some(Command::InstallDesktop),
        #[cfg(target_os = "linux")]
        [c] if c.as_ref() == RESIDENT_ARG => Some(Command::Resident),
        [c, key] if c.as_ref() == "hotkey" && !key.as_ref().trim().is_empty() => {
            Some(Command::Hotkey(key.as_ref().to_string()))
        }
        _ => None,
    }
}

/// Initial startup has no implicit action, and activation bootstrap is never queued.
pub(crate) fn startup_command<S: AsRef<str>>(args: &[S]) -> Option<Command> {
    operation(parse(args))
}

/// Ordinary secondary invocations default to Show, unlike service bootstrapping.
pub(crate) fn secondary_command<S: AsRef<str>>(args: &[S]) -> Option<Command> {
    operation(Some(parse(args).unwrap_or(Command::Show)))
}

/// App actions and activation bootstrap must not open a warning window on startup.
pub(crate) fn allows_startup_warning<S: AsRef<str>>(args: &[S]) -> bool {
    match parse(args) {
        Some(Command::Hotkey(_)) => false,
        #[cfg(target_os = "linux")]
        Some(Command::Resident) => false,
        _ => true,
    }
}

fn operation(command: Option<Command>) -> Option<Command> {
    match command {
        #[cfg(target_os = "linux")]
        Some(Command::Resident) => None,
        command => command,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_warning_visibility_follows_shared_parser() {
        assert!(!allows_startup_warning(&["hotkey", "F11"]));
        #[cfg(target_os = "linux")]
        assert!(!allows_startup_warning(&[RESIDENT_ARG]));
        assert!(allows_startup_warning(&["show"]));
        assert!(allows_startup_warning(&["toggle"]));
        assert!(allows_startup_warning::<&str>(&[]));
    }

    #[test]
    fn parses_known_commands() {
        assert_eq!(parse(&["show"]), Some(Command::Show));
        assert_eq!(parse(&["toggle"]), Some(Command::Toggle));
        assert_eq!(parse(&["install-desktop"]), Some(Command::InstallDesktop));
        assert_eq!(
            parse(&["hotkey", "Ctrl+F1"]),
            Some(Command::Hotkey("Ctrl+F1".to_string()))
        );
    }

    #[test]
    fn rejects_unknown_or_malformed() {
        let none: [&str; 0] = [];
        assert_eq!(parse(&none), None);
        assert_eq!(parse(&["hotkey"]), None);
        assert_eq!(parse(&["hotkey", " "]), None);
        assert_eq!(parse(&["hotkey", "F1", "extra"]), None);
        assert_eq!(parse(&["--whatever"]), None);
        assert_eq!(parse(&["toggle", "extra"]), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bootstrap_is_not_an_initial_or_secondary_operation() {
        assert_eq!(parse(&[RESIDENT_ARG]), Some(Command::Resident));
        assert_eq!(startup_command(&[RESIDENT_ARG]), None);
        assert_eq!(secondary_command(&[RESIDENT_ARG]), None);
        let empty: [&str; 0] = [];
        assert_eq!(startup_command(&empty), None);
        assert_eq!(secondary_command(&empty), Some(Command::Show));
        for command in ["show", "toggle"] {
            assert_eq!(startup_command(&[command]), parse(&[command]));
            assert_eq!(secondary_command(&[command]), parse(&[command]));
        }
    }
}
