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

/// Escape backslash and double-quote for an AppleScript string literal.
fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
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
        // without a title-setting prompt theme. `quoted form of` makes the
        // path shell-safe. The literal "\033]0;...\007" is interpreted by
        // printf as the OSC 0 title sequence; we avoid raw 0x1B bytes
        // because the terminal consumes them before the shell parses.
        let title_part = format!("\\\\033]0;{}\\\\007", escape_applescript(title));
        let script = format!(
            "tell application \"Terminal\"\nactivate\nset newTab to do script (\"printf '{}'; cd \" & quoted form of \"{}\")\ndelay 0.5\ndo script \"{}\" in newTab\nend tell",
            title_part,
            escape_applescript(dir),
            escape_applescript(command)
        );
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
        let ps_command = format!(
            "$Host.UI.RawUI.WindowTitle = '{}'; Set-Location '{}'; {}",
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
