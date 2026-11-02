use std::process::Command;

/// A terminal knows how to open a new window, cd into a directory,
/// and run a command in it. `title` sets the window/tab title so the
/// user can tell which project a window belongs to.
pub trait TerminalAdapter {
    fn launch(&self, title: &str, dir: &str, command: &str) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
pub struct MacTerminal;

#[cfg(target_os = "windows")]
pub struct WindowsPowerShell;

/// Escape a string for embedding in an AppleScript string literal.
///
/// AppleScript literals are terminated by `"` and support the escapes
/// `\\`, `\"`, `\n`, `\r`, `\t`. Raw newlines/returns/tabs cannot appear
/// verbatim in a literal (a line break ends the statement), so they are
/// mapped to their escape sequences, preserving the runtime value.
#[cfg(target_os = "macos")]
fn escape_applescript(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Strip control characters from a window/tab title.
///
/// A title is display-only text that gets printed inside an OSC escape
/// sequence; a raw ESC/BEL in it could terminate the sequence early and
/// inject terminal control codes. Dropping control chars keeps the title
/// human-readable and inert.
#[cfg(target_os = "macos")]
fn sanitize_title(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

/// The AppleScript expression evaluating to the shell command for step 1
/// (set title + cd). Extracted so tests can run the exact expression
/// through osascript + /bin/sh — the same chain Terminal.app performs.
#[cfg(target_os = "macos")]
fn setup_expression(title: &str, dir: &str) -> String {
    format!(
        "\"printf '\\\\033]0;%s\\\\007' \" & quoted form of \"{}\" & \"; cd \" & quoted form of \"{}\"",
        escape_applescript(&sanitize_title(title)),
        escape_applescript(dir)
    )
}

/// Build the AppleScript that opens a Terminal tab, cds into `dir`, sets
/// the window title, and runs `command`.
///
/// Injection safety: no user-controlled value is ever interpolated into a
/// shell-quoted context by hand. Every value reaches the shell exclusively
/// through AppleScript's `quoted form of`, which yields a POSIX
/// single-quoted string immune to `'`, `;`, `$(...)`, backticks, newlines,
/// etc. The AppleScript layer is protected by [`escape_applescript`]. The
/// printf format string is fixed (`\033]0;%s\007`) and contains no user
/// data, so `%` or `\` in the title cannot be reinterpreted.
#[cfg(target_os = "macos")]
fn build_launch_script(title: &str, dir: &str, command: &str) -> String {
    format!(
        "tell application \"Terminal\"\nactivate\nset newTab to do script ({})\ndelay 0.5\ndo script \"{}\" in newTab\nend tell",
        setup_expression(title, dir),
        escape_applescript(command)
    )
}

#[cfg(target_os = "macos")]
impl TerminalAdapter for MacTerminal {
    fn launch(&self, title: &str, dir: &str, command: &str) -> Result<(), String> {
        // Two-step launch, replicating manual use:
        //
        // 1. Open a new tab, cd into the project, set an OSC title hint, and
        //    let the shell render ONE prompt. Zsh themes like powerlevel10k
        //    set the window title to the current directory in their precmd
        //    hook — but that hook only fires when a prompt is drawn. If we
        //    send "cd ... && agent" as one line, no prompt is drawn between
        //    cd and the agent, so p10k never sets the directory title, and
        //    Terminal.app falls back to its device name ("jiangfeng").
        //
        // 2. After a short delay (prompt rendered + p10k title applied),
        //    send the agent command into the same tab.
        //
        // We also set the title via OSC in step 1 as a fallback for users
        // without a title-setting prompt theme. The literal "\033]0;...\007"
        // in the fixed printf format is interpreted by printf as the OSC 0
        // title sequence; we avoid raw 0x1B bytes because the terminal
        // consumes them before the shell parses.
        let script = build_launch_script(title, dir, command);
        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map_err(|e| format!("failed to run osascript: {}", e))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
impl TerminalAdapter for WindowsPowerShell {
    fn launch(&self, title: &str, dir: &str, command: &str) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_CONSOLE: give the process its own window (termana is a
        // GUI app with no console of its own).
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        // No -NoProfile: let the PowerShell profile load so the environment
        // matches the user's normal terminal (fnm / volta PATH, etc.).
        // -NoExit: keep the window open after the agent exits.
        // Set the window title first so the user can identify the project;
        // escape single quotes in the title by doubling them.
        //
        // Refresh PATH from the registry (Machine + User) before running the
        // agent command. When a GUI app is launched (e.g. via File Explorer
        // or Tauri), it inherits a minimal PATH that may miss toolchains
        // installed via fnm/volta/hermes. `$env:Path` is rebuilt from the
        // registry so the spawned PowerShell window sees the same PATH the
        // user gets in a normal terminal.
        let ps_command = format!(
            "$m = [Environment]::GetEnvironmentVariable('Path','Machine'); \
             $u = [Environment]::GetEnvironmentVariable('Path','User'); \
             $env:Path = \"$m;$u\"; \
             $Host.UI.RawUI.WindowTitle = '{}'; \
             Set-Location '{}'; \
             {}",
            title.replace('\'', "''"),
            dir.replace('\'', "''"),
            command
        );
        Command::new("powershell")
            .arg("-NoExit")
            .arg("-Command")
            .arg(&ps_command)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
            .map_err(|e| format!("failed to launch powershell: {}", e))?;
        Ok(())
    }
}

pub fn default_terminal() -> Box<dyn TerminalAdapter> {
    #[cfg(target_os = "macos")]
    {
        Box::new(MacTerminal)
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(WindowsPowerShell)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        compile_error!("termana currently supports only macOS and Windows");
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn escape_applescript_covers_quotes_backslashes_and_control_chars() {
        assert_eq!(escape_applescript("plain"), "plain");
        assert_eq!(escape_applescript("a\"b"), "a\\\"b");
        assert_eq!(escape_applescript("a\\b"), "a\\\\b");
        assert_eq!(escape_applescript("a\nb"), "a\\nb");
        assert_eq!(escape_applescript("a\rb"), "a\\rb");
        assert_eq!(escape_applescript("a\tb"), "a\\tb");
        // Single quotes need no AppleScript-layer escaping: `quoted form of`
        // handles them at the shell layer.
        assert_eq!(escape_applescript("Bob's"), "Bob's");
    }

    #[test]
    fn sanitize_title_strips_control_characters() {
        assert_eq!(sanitize_title("Bob's App"), "Bob's App");
        assert_eq!(sanitize_title("a\u{7}b\u{1b}c\nd\re"), "abcde");
    }

    #[test]
    fn launch_script_matches_golden_output() {
        let script = build_launch_script("My App — Claude Code", "/Users/me/My App", "claude");
        let expected = "tell application \"Terminal\"\nactivate\nset newTab to do script (\"printf '\\\\033]0;%s\\\\007' \" & quoted form of \"My App — Claude Code\" & \"; cd \" & quoted form of \"/Users/me/My App\")\ndelay 0.5\ndo script \"claude\" in newTab\nend tell";
        assert_eq!(script, expected);
    }

    /// Structural guard: user-controlled values may only ever appear inside
    /// `quoted form of "..."`; no hand-rolled shell quoting may creep back.
    #[test]
    fn launch_script_routes_user_input_through_quoted_form_of() {
        let title = "x'; touch /tmp/termana_poc_struct; echo '";
        let dir = "/tmp/d'; touch /tmp/termana_poc_struct; echo '";
        let script = build_launch_script(title, dir, "agent --flag");
        assert!(
            script.contains(&format!("quoted form of \"{}\"", escape_applescript(title))),
            "title not quoted: {script}"
        );
        assert!(
            script.contains(&format!("quoted form of \"{}\"", escape_applescript(dir))),
            "dir not quoted: {script}"
        );
        // The fixed printf format is the only single-quoted shell fragment.
        assert!(
            !script.contains("printf '{"),
            "format holds user data: {script}"
        );
    }

    /// Compile (not run) the full script for hostile inputs: escaping bugs
    /// would break compilation; we never launch Terminal.app in tests.
    #[test]
    fn launch_script_compiles_for_hostile_inputs() {
        let script = build_launch_script(
            "a'b\"c\\d\ne",
            "/tmp/x'y\"z\\w\nq",
            "claude --say \"hi 'there'\"",
        );
        let dir = tempfile::tempdir().unwrap();
        let out = Command::new("osacompile")
            .arg("-e")
            .arg(&script)
            .arg("-o")
            .arg(dir.path().join("probe.scpt"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "osacompile rejected script: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// End-to-end proof over the real execution chain: osascript evaluates
    /// the exact `do script` argument, then /bin/sh runs the resulting
    /// string — the same POSIX quoting zsh applies. If any layer misquoted,
    /// the payloads would create probe files in the shell's working
    /// directory. (Payloads use relative paths so the directory names under
    /// test contain no literal `/`.)
    #[test]
    fn malicious_names_cannot_inject_shell_commands() {
        let pocs = [
            "termana_poc_semi",
            "termana_poc_dollar",
            "termana_poc_title",
            "termana_poc_newline",
            "termana_poc_backslash",
        ];
        let cwd = std::env::current_dir().unwrap();
        for poc in &pocs {
            let _ = std::fs::remove_file(cwd.join(poc));
        }

        let root = tempfile::tempdir().unwrap();
        let dirs = [
            "x'; touch termana_poc_semi; echo '",
            "$(touch termana_poc_dollar)",
            "line1\nline2'; touch termana_poc_newline; echo '",
            "back\\slash'; touch termana_poc_backslash; echo '",
        ];
        let title = "Bob's $(touch termana_poc_title); echo '";

        for name in &dirs {
            let dir = root.path().join(name);
            std::fs::create_dir(&dir).unwrap();
            let dir = dir.to_string_lossy().into_owned();

            // Step 1: osascript evaluates the real expression.
            let evaluated = Command::new("osascript")
                .arg("-e")
                .arg(setup_expression(title, &dir))
                .output()
                .unwrap();
            assert!(
                evaluated.status.success(),
                "osascript: {}",
                String::from_utf8_lossy(&evaluated.stderr)
            );
            let shell_cmd = String::from_utf8(evaluated.stdout)
                .unwrap()
                .trim_end()
                .to_string();

            // Step 2: the shell executes it — exit status comes from `cd`,
            // so success also proves the hostile path arrived as ONE argument.
            let run = Command::new("/bin/sh")
                .arg("-c")
                .arg(&shell_cmd)
                .output()
                .unwrap();
            assert!(
                run.status.success(),
                "shell failed for {dir:?}: {}",
                String::from_utf8_lossy(&run.stderr)
            );
        }

        for poc in &pocs {
            assert!(
                !Path::new(&cwd.join(poc)).exists(),
                "injection executed: {poc}"
            );
        }
    }
}
