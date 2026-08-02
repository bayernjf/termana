use std::process::Command;

/// A terminal knows how to open a new window, cd into a directory,
/// and run a command in it.
pub trait TerminalAdapter {
    fn launch(&self, dir: &str, command: &str) -> Result<(), String>;
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
    fn launch(&self, dir: &str, command: &str) -> Result<(), String> {
        // `do script` sends the text to a new interactive Terminal window.
        // `quoted form of` wraps the path in shell-safe single quotes so
        // spaces and odd characters don't break the cd. After the agent
        // exits, the shell remains open in the project directory.
        let script = format!(
            "tell application \"Terminal\"\nactivate\ndo script \"cd \" & quoted form of \"{}\" & \" && {}\"\nend tell",
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
    fn launch(&self, dir: &str, command: &str) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        // CREATE_NEW_CONSOLE: give the process its own window (termana is a
        // GUI app with no console of its own).
        const CREATE_NEW_CONSOLE: u32 = 0x00000010;
        // No -NoProfile: let the PowerShell profile load so the environment
        // matches the user's normal terminal (fnm / volta PATH, etc.).
        // -NoExit: keep the window open after the agent exits.
        // Single-quote the path; double any embedded single quote.
        let ps_command = format!("Set-Location '{}'; {}", dir.replace('\'', "''"), command);
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
