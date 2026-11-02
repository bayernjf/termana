use std::process::Stdio;

/// Extract the executable file name from a launch command string.
///
/// Commands may carry flags or arguments (`"gemini --yolo"`, `"codex exec"`)
/// that `command -v` / `Get-Command` would misread as part of the program
/// name; only the first token is an executable. A quoted name
/// (`"\"my agent\" --flag"`) is unquoted. Absolute paths are returned as-is.
/// Empty input yields an empty string (which `installed_status` reports as
/// not installed).
pub fn executable_name(command: &str) -> &str {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return "";
    }
    if trimmed.starts_with('"') || trimmed.starts_with('\'') {
        let quote = trimmed.chars().next().unwrap();
        let mut escaped = false;
        for (i, c) in trimmed.char_indices().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == quote {
                return &trimmed[1..i];
            }
        }
        // Unterminated quote: fall back to the first whitespace token.
        return trimmed
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c| c == '"' || c == '\'');
    }
    trimmed.split_whitespace().next().unwrap_or("")
}

/// Quote a string for POSIX shell single-quote context (`'` → `'\''`).
///
/// Anything inside single quotes is inert to the shell — no substitution,
/// no globbing, no word splitting — so this safely embeds an arbitrary
/// executable name into a `-c` script.
#[cfg(unix)]
fn sh_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Check whether each command is available on PATH, in a SINGLE shell call.
///
/// Only the executable name of each command is probed (see
/// [`executable_name`]), so flags and arguments in a command string do not
/// break detection.
///
/// Runs inside the same kind of shell the launched terminal uses, so PATH
/// setup from rc / profile files (fnm, nvm, asdf, volta, ...) is applied:
///   - unix:    `$SHELL -ilc` (login interactive, sources .zprofile + .zshrc)
///   - windows: `powershell -Command` (loads the PowerShell profile)
///
/// A plain `which` / `where` would miss tools whose PATH is set only in an
/// rc or profile -- especially when termana runs as a GUI app (no rc
/// sourced).
///
/// Only external commands count (an absolute path on unix, an Application
/// on windows), so shell builtins / keywords (e.g. `continue`) and aliases
/// are not mistaken for installed agents. Each command prints `1` (found)
/// or `0` (not) on its own line.
pub fn installed_status(commands: &[&str]) -> Vec<bool> {
    let n = commands.len();
    if n == 0 {
        return vec![];
    }
    let executables: Vec<&str> = commands.iter().map(|c| executable_name(c)).collect();

    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let script: String = executables
            .iter()
            .map(|c| {
                format!(
                    "p=$(command -v {c} 2>/dev/null); case \"$p\" in /*) printf '1\\n';; *) printf '0\\n';; esac",
                    c = sh_single_quote(c)
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        let out = std::process::Command::new(&shell)
            .arg("-ilc")
            .arg(&script)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        parse_flags(out, n)
    }

    #[cfg(windows)]
    {
        let list = executables
            .iter()
            .map(|c| format!("'{}'", c.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        let script = format!(
            "@({list}) | ForEach-Object {{ if (Get-Command $_ -CommandType Application -ErrorAction SilentlyContinue) {{ '1' }} else {{ '0' }} }}"
        );
        let out = std::process::Command::new("powershell")
            .arg("-Command")
            .arg(&script)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        parse_flags(out, n)
    }

    #[cfg(not(any(unix, windows)))]
    {
        vec![false; n]
    }
}

/// Parse `1` / `0` lines into bools, padded / truncated to `n`.
fn parse_flags(out: Result<std::process::Output, std::io::Error>, n: usize) -> Vec<bool> {
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout);
            let mut flags: Vec<bool> = s
                .lines()
                .map(|l| l.trim())
                .filter(|l| *l == "1" || *l == "0")
                .map(|l| l == "1")
                .collect();
            flags.resize(n, false);
            flags
        }
        Err(_) => vec![false; n],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_name_takes_the_first_token() {
        assert_eq!(executable_name("gemini"), "gemini");
        assert_eq!(executable_name("gemini --yolo"), "gemini");
        assert_eq!(executable_name("codex exec --threads 4"), "codex");
        assert_eq!(executable_name("/usr/local/bin/agent --flag"), "/usr/local/bin/agent");
    }

    #[test]
    fn executable_name_unquotes_a_quoted_name() {
        assert_eq!(executable_name("\"my agent\" --yolo"), "my agent");
        assert_eq!(executable_name("'my agent'"), "my agent");
    }

    #[test]
    fn executable_name_handles_empty_and_whitespace() {
        assert_eq!(executable_name(""), "");
        assert_eq!(executable_name("   "), "");
        assert_eq!(executable_name("\t\n"), "");
    }

    #[cfg(unix)]
    #[test]
    fn installed_status_probes_the_executable_not_the_flags() {
        // `/bin/ls` exists on every POSIX box; the flag variant must resolve
        // to the same binary instead of being misread as a program named
        // `ls -la`. The third command is deliberately nonexistent.
        let flags = installed_status(&[
            "/bin/ls",
            "ls -la",
            "termana-no-such-command-9f3b2",
        ]);
        assert_eq!(flags, vec![true, true, false]);
    }

    #[cfg(unix)]
    #[test]
    fn installed_status_cannot_be_injected_via_executable_name() {
        // `executable_name` strips the outer quotes, leaving the payload as
        // the probed "executable"; `command -v` must see it as inert data,
        // never as shell to evaluate.
        let pocs = [
            "/tmp/termana_poc_agent_subst",
            "/tmp/termana_poc_agent_backtick",
            "/tmp/termana_poc_agent_semi",
        ];
        for poc in &pocs {
            let _ = std::fs::remove_file(poc);
        }

        let flags = installed_status(&[
            "'$(touch /tmp/termana_poc_agent_subst)'",
            "'`touch /tmp/termana_poc_agent_backtick`'",
            "\"; touch /tmp/termana_poc_agent_semi; echo '",
        ]);
        assert_eq!(flags, vec![false, false, false]);

        for poc in &pocs {
            assert!(
                !std::path::Path::new(poc).exists(),
                "injection executed: {poc}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn sh_single_quote_wraps_single_quotes_posix_style() {
        assert_eq!(sh_single_quote("plain"), "'plain'");
        assert_eq!(sh_single_quote("a'b"), "'a'\\''b'");
        assert_eq!(sh_single_quote("$(x); `y`"), "'$(x); `y`'");
    }
}
