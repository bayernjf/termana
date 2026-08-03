use std::process::Stdio;

/// Check whether each command is available on PATH, in a SINGLE shell call.
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

    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let script: String = commands
            .iter()
            .map(|c| {
                format!(
                    "p=$(command -v {c} 2>/dev/null); case \"$p\" in /*) printf '1\\n';; *) printf '0\\n';; esac"
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
        let list = commands
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
