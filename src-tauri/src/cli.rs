//! Command line of a running/starting shun.
//!
//! A second `shun ...` invocation is forwarded to the resident instance (single-instance
//! plugin), so these double as
//! a way to trigger the global-hotkey actions from outside — e.g. from a compositor bind.
//!
//! ```text
//! shun                  show the launcher
//! shun show             show the launcher (also when starting fresh)
//! shun toggle           show / hide the launcher (the `launch` hotkey)
//! shun hotkey <KEY>     run the `[[apps]]` entry whose `hotkey` is <KEY> (e.g. "F10")
//! shun install-desktop  Linux: write the .desktop entry + icon, then exit
//! ```

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Show,
    Toggle,
    /// A hotkey as written in `config.toml`.
    Hotkey(String),
    InstallDesktop,
}

/// Parses the arguments after the program name. `None` for no/unknown arguments.
pub fn parse<S: AsRef<str>>(args: &[S]) -> Option<Command> {
    match args {
        [c] if c.as_ref() == "show" => Some(Command::Show),
        [c] if c.as_ref() == "toggle" => Some(Command::Toggle),
        [c] if c.as_ref() == "install-desktop" => Some(Command::InstallDesktop),
        [c, key] if c.as_ref() == "hotkey" && !key.as_ref().trim().is_empty() => {
            Some(Command::Hotkey(key.as_ref().to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
