use crate::models::BannerMode;
use std::io::IsTerminal;

pub const OWL_BANNER: &str = r#"⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣀⣄⣀⡀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠐⣶⣾⣿⣿⣿⣿⣿⣶⡆⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⢰⡏⢤⡎⣿⣿⢡⣶⢹⣧⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣶⣶⣇⣸⣷⣶⣾⣿⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⢨⣿⣿⣿⢟⣿⣿⣿⣿⣿⣧⡀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣿⡏⣿⣿⣿⣿⣿⣿⣿⣿⡄⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠘⣿⣿⣿⣜⠿⣿⣿⣿⣿⣿⣿⣿⡄⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠐⣷⣿⡿⣷⣮⣙⠿⣿⣿⣿⣿⣿⡄⠀
⠀⠀⠠⢄⣀⡀⠀⠀⠀⠀⠀⠈⠫⡯⢿⣿⣿⣿⣶⣯⣿⣻⣿⣿⠀
⠀⠀⠤⢆⠆⠈⠉⠳⠤⣄⡀⠀⠀⠀⠙⢻⣿⣿⠿⠿⠿⢻⣿⠙⠇
⠠⠤⠀⣉⣁⣢⣄⣀⣀⣤⣿⠷⠦⠤⣠⡶⠿⣟⠀⠀⠀⠀⠻⡀⠀
⠀⠀⠔⠋⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠃⠃⠉⠉⠛⠛⠿⢷⡶⠀⠀"#;

/// Pure predicate deciding whether the owl banner should print.
///
/// Hard suppressors (any one disables the banner regardless of mode):
/// - the command is not the interactive `chat` shell,
/// - machine-readable `--json` output is requested,
/// - the banner is explicitly disabled via `--no-banner`.
///
/// Otherwise the `BannerMode` governs:
/// - `Never` suppresses,
/// - `Always` forces (even when stdout is redirected or under CI),
/// - `Auto` prints only on an interactive TTY that is not a CI environment.
pub fn should_print_banner(
    mode: BannerMode,
    is_chat: bool,
    stdout_is_tty: bool,
    json: bool,
    no_banner: bool,
    ci: bool,
) -> bool {
    if !is_chat || json || no_banner {
        return false;
    }
    match mode {
        BannerMode::Never => false,
        BannerMode::Always => true,
        BannerMode::Auto => stdout_is_tty && !ci,
    }
}

/// Environment-reading wrapper around [`should_print_banner`]. Reads the real
/// stdout TTY state and the `CI` environment variable.
pub fn should_print_banner_env(
    mode: BannerMode,
    is_chat: bool,
    json: bool,
    no_banner: bool,
) -> bool {
    let stdout_is_tty = std::io::stdout().is_terminal();
    let ci = std::env::var_os("CI").is_some();
    should_print_banner(mode, is_chat, stdout_is_tty, json, no_banner, ci)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_prints_on_interactive_tty() {
        assert!(should_print_banner(
            BannerMode::Auto,
            true,
            true,
            false,
            false,
            false
        ));
    }

    #[test]
    fn auto_suppressed_when_redirected() {
        assert!(!should_print_banner(
            BannerMode::Auto,
            true,
            false,
            false,
            false,
            false
        ));
    }

    #[test]
    fn auto_suppressed_under_ci() {
        assert!(!should_print_banner(
            BannerMode::Auto,
            true,
            true,
            false,
            false,
            true
        ));
    }

    #[test]
    fn always_prints_even_when_redirected_or_ci() {
        assert!(should_print_banner(
            BannerMode::Always,
            true,
            false,
            false,
            false,
            true
        ));
    }

    #[test]
    fn never_suppresses_even_on_tty() {
        assert!(!should_print_banner(
            BannerMode::Never,
            true,
            true,
            false,
            false,
            false
        ));
    }

    #[test]
    fn json_suppresses_regardless_of_mode() {
        assert!(!should_print_banner(
            BannerMode::Always,
            true,
            true,
            true,
            false,
            false
        ));
    }

    #[test]
    fn no_banner_suppresses_regardless_of_mode() {
        assert!(!should_print_banner(
            BannerMode::Always,
            true,
            true,
            false,
            true,
            false
        ));
    }

    #[test]
    fn non_chat_commands_never_print() {
        assert!(!should_print_banner(
            BannerMode::Always,
            false,
            true,
            false,
            false,
            false
        ));
    }
}
