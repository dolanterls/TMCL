//! Export a launch command as a standalone script.
//!
//! A script is what a player attaches to a bug report: it reproduces the exact
//! classpath, JVM flags and main class without asking them to read a log. The
//! access token is never written to disk - it is replaced by an environment
//! variable reference, so the exported file can be shared.

use std::path::Path;

use crate::core::launch::command::CommandSpec;

/// Environment variable the exported script reads the session token from.
pub const TOKEN_VARIABLE: &str = "TMCL_ACCESS_TOKEN";

/// Stand-in used while building the command, replaced by the shell's syntax
/// for reading [`TOKEN_VARIABLE`].
pub const TOKEN_SENTINEL: &str = "__TMCL_ACCESS_TOKEN__";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptShell {
    Batch,
    PowerShell,
    Bash,
}

impl ScriptShell {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "bat" | "batch" | "cmd" => Some(Self::Batch),
            "ps1" | "powershell" | "pwsh" => Some(Self::PowerShell),
            "sh" | "bash" => Some(Self::Bash),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Batch => "bat",
            Self::PowerShell => "ps1",
            Self::Bash => "sh",
        }
    }

    /// The shell a fresh export targets on this platform.
    pub fn for_host() -> Self {
        if cfg!(windows) {
            Self::Batch
        } else {
            Self::Bash
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedScript {
    pub contents: String,
    /// Things the player should know before running it.
    pub warnings: Vec<String>,
}

/// Renders `spec` for `shell`, replacing every occurrence of `token` with a
/// reference to [`TOKEN_VARIABLE`].
///
/// An offline session has no token at all, so an export that does not contain
/// one must not mention the variable either: a warning about an unset token
/// sends the reader looking for something that never existed.
pub fn render(spec: &CommandSpec, shell: ScriptShell, token: &str) -> RenderedScript {
    let token_reference = match shell {
        ScriptShell::Batch => format!("%{TOKEN_VARIABLE}%"),
        ScriptShell::PowerShell => format!("$env:{TOKEN_VARIABLE}"),
        ScriptShell::Bash => format!("\"${{{TOKEN_VARIABLE}}}\""),
    };
    let mut references_token = false;
    let args = spec
        .args
        .iter()
        .map(|argument| {
            let argument = if token.is_empty() {
                argument.clone()
            } else if argument == token {
                // The session token is always its own argument. Embedded
                // occurrences (a custom `--auth=...`) cannot be replaced by an
                // environment reference without splitting the quotes, so they
                // are redacted instead of leaking into a shared file.
                references_token = true;
                TOKEN_SENTINEL.to_owned()
            } else {
                argument.replace(token, "<redacted>")
            };
            // Path arguments (`-cp`, `--gameDir`, `-Djava.library.path`) are
            // joined here, so the prefix has to go wherever it starts a
            // segment, not only at the front of the argument.
            without_verbatim_prefix(&argument)
        })
        .collect::<Vec<_>>();

    let mut warnings = Vec::new();
    let command_length = spec.program.to_string_lossy().len().saturating_add(
        args.iter()
            .map(|argument| argument.len() + 3)
            .sum::<usize>(),
    );
    match shell {
        // cmd.exe stops at 8191 characters per line, well below the 32767 the
        // Windows process API allows.
        ScriptShell::Batch if command_length > 8_000 => warnings.push(
            "the command line is close to the 8191-character limit of cmd.exe; export a .ps1 instead"
                .to_owned(),
        ),
        ScriptShell::PowerShell if command_length > 30_000 => warnings.push(
            "the command line is close to the 32767-character limit of CreateProcess".to_owned(),
        ),
        _ => {}
    }

    // Only an export that substitutes a token gets the environment reference and
    // its "not set" warning.
    let reference = references_token.then_some(token_reference);
    let reference = reference.as_deref();
    let contents = match shell {
        ScriptShell::Batch => batch(spec, &args, reference),
        ScriptShell::PowerShell => powershell(spec, &args, reference),
        ScriptShell::Bash => bash(spec, &args, reference),
    };
    RenderedScript { contents, warnings }
}

fn batch(spec: &CommandSpec, args: &[String], token: Option<&str>) -> String {
    let mut out = String::from("@echo off\r\n");
    // Paths in the exported file are UTF-8; cmd would otherwise decode them in
    // the console code page and mangle non-ASCII directory names.
    out.push_str("chcp 65001 >nul\r\n");
    out.push_str("rem Generated by TMCL.");
    if token.is_some() {
        out.push_str(" The session token is read from ");
        out.push_str(TOKEN_VARIABLE);
        out.push('.');
    }
    out.push_str("\r\n");
    if token.is_some() {
        out.push_str(&format!(
            "if not defined {TOKEN_VARIABLE} echo [tmcl] {TOKEN_VARIABLE} is not set; the session will be invalid. 1>&2\r\n"
        ));
    }
    out.push_str(&format!(
        "cd /d {}\r\n",
        batch_quote(&display(&spec.current_dir))
    ));
    out.push_str(&batch_quote(&display(&spec.program)));
    for argument in args {
        out.push(' ');
        out.push_str(&batch_quote(argument));
    }
    out.push_str("\r\n");
    match token {
        Some(reference) => out.replace(TOKEN_SENTINEL, reference),
        None => out,
    }
}

fn powershell(spec: &CommandSpec, args: &[String], token: Option<&str>) -> String {
    let mut out = String::from("# Generated by TMCL.");
    if token.is_some() {
        out.push_str(" The session token is read from ");
        out.push_str(TOKEN_VARIABLE);
        out.push('.');
    }
    out.push('\n');
    if token.is_some() {
        out.push_str(&format!(
            "if (-not $env:{TOKEN_VARIABLE}) {{ Write-Warning '{TOKEN_VARIABLE} is not set; the session will be invalid.' }}\n"
        ));
    }
    out.push_str(&format!(
        "Set-Location -LiteralPath {}\n",
        powershell_quote(&display(&spec.current_dir))
    ));
    out.push_str("$tmclArguments = @(\n");
    for argument in args {
        out.push_str("    ");
        out.push_str(&powershell_quote(argument));
        out.push('\n');
    }
    out.push_str(")\n");
    out.push_str("& ");
    out.push_str(&powershell_quote(&display(&spec.program)));
    out.push_str(" $tmclArguments\n");
    match token {
        Some(reference) => out.replace(&powershell_quote(TOKEN_SENTINEL), reference),
        None => out,
    }
}

fn bash(spec: &CommandSpec, args: &[String], token: Option<&str>) -> String {
    let mut out = String::from("#!/usr/bin/env bash\n");
    out.push_str("# Generated by TMCL.");
    if token.is_some() {
        out.push_str(" The session token is read from ");
        out.push_str(TOKEN_VARIABLE);
        out.push('.');
    }
    out.push('\n');
    if token.is_some() {
        out.push_str(&format!(
            ": \"${{{TOKEN_VARIABLE}:?set {TOKEN_VARIABLE} to the account access token}}\"\n"
        ));
    }
    out.push_str("cd -- ");
    out.push_str(&bash_quote(&display(&spec.current_dir)));
    out.push('\n');
    out.push_str("exec ");
    out.push_str(&bash_quote(&display(&spec.program)));
    for argument in args {
        out.push_str(" \\\n  ");
        out.push_str(&bash_quote(argument));
    }
    out.push('\n');
    match token {
        Some(reference) => out.replace(&bash_quote(TOKEN_SENTINEL), reference),
        None => out,
    }
}

/// Batch quoting: `%` and `!` are expanded even inside double quotes, so a
/// literal percent has to be doubled. Arguments come from paths and UUIDs;
/// none of them legitimately contain a quote.
fn batch_quote(value: &str) -> String {
    if value.is_empty() {
        return "\"\"".to_owned();
    }
    let escaped = value.replace('%', "%%");
    if escaped.chars().any(|character| {
        matches!(
            character,
            ' ' | '"' | '&' | '|' | '<' | '>' | '^' | '(' | ')'
        )
    }) {
        format!("\"{}\"", escaped.replace('"', "\"\""))
    } else {
        escaped
    }
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn bash_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn display(path: &Path) -> String {
    without_verbatim_prefix(&crate::core::instance::path::display_path(path))
}

/// Drops the Windows verbatim prefix from a path or from every path inside a
/// joined value (`-cp a.jar;b.jar`).
///
/// `\\?\` only exists to opt a Win32 call into long paths; a shell treats it as
/// a literal directory name, so `cmd /c "\\?\C:\...\java.exe" -version` fails
/// with exit 3 while the same path without the prefix runs. The prefix can also
/// sit in the middle of an argument, hence the search rather than a
/// `strip_prefix`.
fn without_verbatim_prefix(value: &str) -> String {
    if !cfg!(windows) || !value.contains(r"\\?\") {
        return value.to_owned();
    }
    value.replace(r"\\?\UNC\", r"\\").replace(r"\\?\", "")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ScriptShell, TOKEN_SENTINEL, TOKEN_VARIABLE};
    use crate::core::launch::command::CommandSpec;

    fn spec() -> CommandSpec {
        CommandSpec {
            program: PathBuf::from(r"C:\Program Files\Java\jdk-21\bin\javaw.exe"),
            args: vec![
                "-Xmx4G".into(),
                "-cp".into(),
                r"C:\Users\me\.minecraft\libraries\a.jar;C:\Users\me\.minecraft\versions\pack\pack.jar"
                    .into(),
                "net.minecraft.client.main.Main".into(),
                "--accessToken".into(),
                "secret-token".into(),
                "--username".into(),
                "Player".into(),
            ],
            classpath: String::new(),
            current_dir: PathBuf::from(r"C:\Users\me\.minecraft\versions\pack"),
        }
    }

    #[test]
    fn the_batch_script_quotes_paths_and_reads_the_token_from_the_environment() {
        let rendered = super::render(&spec(), ScriptShell::Batch, "secret-token");
        let text = &rendered.contents;
        assert!(text.starts_with("@echo off\r\n"), "{text}");
        assert!(text.contains("chcp 65001"), "{text}");
        assert!(
            text.contains(r"cd /d C:\Users\me\.minecraft\versions\pack"),
            "{text}"
        );
        assert!(
            text.contains(r#""C:\Program Files\Java\jdk-21\bin\javaw.exe""#),
            "program path must stay quoted: {text}"
        );
        assert!(
            text.contains(&format!("%{TOKEN_VARIABLE}%")),
            "token reference missing: {text}"
        );
        assert!(
            !text.contains("secret-token"),
            "the real token must never be written to disk: {text}"
        );
        assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
    }

    #[test]
    fn the_powershell_script_passes_an_argument_array() {
        let rendered = super::render(&spec(), ScriptShell::PowerShell, "secret-token");
        let text = &rendered.contents;
        assert!(text.contains("$tmclArguments = @("), "{text}");
        assert!(
            text.contains("Set-Location -LiteralPath 'C:\\Users\\me\\.minecraft\\versions\\pack'"),
            "{text}"
        );
        assert!(
            text.contains("& 'C:\\Program Files\\Java\\jdk-21\\bin\\javaw.exe' $tmclArguments"),
            "{text}"
        );
        assert!(text.contains(&format!("$env:{TOKEN_VARIABLE}")), "{text}");
        assert!(!text.contains("secret-token"), "{text}");
    }

    #[test]
    fn the_bash_script_execs_with_a_single_quoted_argument_list() {
        let rendered = super::render(&spec(), ScriptShell::Bash, "secret-token");
        let text = &rendered.contents;
        assert!(text.starts_with("#!/usr/bin/env bash\n"), "{text}");
        assert!(
            text.contains(&format!(": \"${{{TOKEN_VARIABLE}:?")),
            "{text}"
        );
        // Single quotes survive a path with an apostrophe.
        let mut spec = spec();
        spec.current_dir = PathBuf::from("/home/o'brien/.minecraft/versions/pack");
        let rendered = super::render(&spec, ScriptShell::Bash, "secret-token");
        assert!(
            rendered
                .contents
                .contains(r"cd -- '/home/o'\''brien/.minecraft/versions/pack'"),
            "{}",
            rendered.contents
        );
        assert!(!text.contains("secret-token"), "{text}");
    }

    #[test]
    fn a_token_inside_another_argument_is_redacted_not_leaked() {
        let mut spec = spec();
        // A custom game argument could carry the token with a prefix; it cannot
        // become an environment reference without splitting the shell quotes,
        // so it must at least never reach the file.
        spec.args.push("--auth=secret-token".into());
        let rendered = super::render(&spec, ScriptShell::Batch, "secret-token");
        assert!(
            rendered.contents.contains("--auth=<redacted>"),
            "{}",
            rendered.contents
        );
        assert!(!rendered.contents.contains("secret-token"));
    }

    #[test]
    fn a_long_command_line_warns_about_the_shell_limit() {
        let mut spec = spec();
        spec.args = vec!["-cp".into(), "x".repeat(9_000)];
        let rendered = super::render(&spec, ScriptShell::Batch, "secret-token");
        assert_eq!(rendered.warnings.len(), 1, "{:?}", rendered.warnings);
        assert!(
            rendered.warnings[0].contains("8191"),
            "{:?}",
            rendered.warnings
        );

        let rendered = super::render(&spec, ScriptShell::PowerShell, "secret-token");
        assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
    }

    #[test]
    fn the_sentinel_is_not_left_in_an_export_without_a_token() {
        let rendered = super::render(&spec(), ScriptShell::Batch, "");
        assert!(!rendered.contents.contains(TOKEN_SENTINEL));
        // With no token to hide, the argument is exported as-is.
        assert!(rendered.contents.contains("secret-token"));
    }

    #[test]
    fn an_offline_export_does_not_mention_the_access_token() {
        // An offline account authenticates with the literal `0`; there is no
        // token to read, so neither the comment nor the "not set" warning may
        // appear - it would point at a variable the file never uses.
        for shell in [
            ScriptShell::Batch,
            ScriptShell::PowerShell,
            ScriptShell::Bash,
        ] {
            let rendered = super::render(&spec(), shell, "");
            assert!(
                !rendered.contents.contains(TOKEN_VARIABLE),
                "{shell:?} mentions the token variable: {}",
                rendered.contents
            );
            assert!(
                !rendered.contents.contains(TOKEN_SENTINEL),
                "{shell:?} left the sentinel: {}",
                rendered.contents
            );
        }
    }

    #[test]
    fn a_microsoft_export_still_asks_for_the_token() {
        for shell in [
            ScriptShell::Batch,
            ScriptShell::PowerShell,
            ScriptShell::Bash,
        ] {
            let rendered = super::render(&spec(), shell, "secret-token");
            assert!(
                rendered.contents.contains(TOKEN_VARIABLE),
                "{shell:?} lost the token reference: {}",
                rendered.contents
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn verbatim_prefixes_are_stripped_from_every_path_in_the_script() {
        let mut spec = spec();
        spec.program = PathBuf::from(r"\\?\C:\Program Files\Java\jdk-21\bin\javaw.exe");
        spec.current_dir = PathBuf::from(r"\\?\C:\Users\me\.minecraft\versions\pack");
        // `-cp` joins several paths into one argument, so a prefix in the
        // middle has to be removed as well.
        spec.args[2] = concat!(
            r"\\?\C:\Users\me\.minecraft\libraries\a.jar;",
            r"C:\Users\me\.minecraft\versions\pack\pack.jar"
        )
        .into();

        let rendered = super::render(&spec, ScriptShell::Batch, "secret-token");
        let text = &rendered.contents;
        assert!(!text.contains(r"\\?\"), "{text}");
        assert!(
            text.contains(r#""C:\Program Files\Java\jdk-21\bin\javaw.exe""#),
            "{text}"
        );
        assert!(
            text.contains(r"C:\Users\me\.minecraft\libraries\a.jar;C:\Users\me"),
            "{text}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_verbatim_unc_path_becomes_a_plain_unc_path() {
        assert_eq!(
            super::without_verbatim_prefix(r"\\?\UNC\server\share\java.exe"),
            r"\\server\share\java.exe"
        );
        assert_eq!(
            super::without_verbatim_prefix(r"C:\Java\java.exe"),
            r"C:\Java\java.exe"
        );
    }

    #[test]
    fn script_shells_parse_their_common_spellings() {
        assert_eq!(ScriptShell::parse("bat"), Some(ScriptShell::Batch));
        assert_eq!(ScriptShell::parse("PS1"), Some(ScriptShell::PowerShell));
        assert_eq!(ScriptShell::parse("sh"), Some(ScriptShell::Bash));
        assert_eq!(ScriptShell::parse("fish"), None);
        assert_eq!(ScriptShell::Batch.extension(), "bat");
    }
}
