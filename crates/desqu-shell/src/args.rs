//! Launch arguments the shell understands. Everything else is ignored so apps can add their own.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LaunchArgs {
    /// Set by the Run-key entry written for "start with system".
    pub autostart: bool,
    /// Ask a running instance to exit (restoring managed windows), or exit immediately if none runs.
    pub quit: bool,
}

pub fn parse<I: IntoIterator<Item = String>>(args: I) -> LaunchArgs {
    let mut parsed = LaunchArgs::default();
    for arg in args {
        match arg.as_str() {
            "--autostart" => parsed.autostart = true,
            "--quit" => parsed.quit = true,
            _ => {}
        }
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> LaunchArgs {
        parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_args_is_default() {
        assert_eq!(parse_strs(&[]), LaunchArgs::default());
    }

    #[test]
    fn recognises_both_flags_in_any_order() {
        let a = parse_strs(&["app.exe", "--quit", "--autostart"]);
        assert!(a.autostart && a.quit);
    }

    #[test]
    fn ignores_unknown_and_lookalike_args() {
        let a = parse_strs(&["--autostarted", "-quit", "--verbose", "quit"]);
        assert_eq!(a, LaunchArgs::default());
    }
}
