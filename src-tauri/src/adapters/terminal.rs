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
        // `do script` sends the text to a new interactive Terminal window.
        // `quoted form of` wraps the path in shell-safe single quotes so
        // spaces and odd characters don't break the cd. After the agent
        // exits, the shell remains open in the project directory.
        //
        // Set the window/tab title via Terminal.app's native `custom title`
        // property so the user can see which project a window belongs to.
        // We capture the tab returned by `do script` and set its custom
        // title explicitly — this is more robust than an OSC escape
        // sequence (which the agent's TUI would immediately overwrite).
        // `quoted form of` wraps the path in shell-safe single quotes.
        let script = format!(
            "tell application \"Terminal\"\nactivate\nset newTab to do script (\"cd \" & quoted form of \"{}\" & \" && {}\")\nset custom title of newTab to \"{}\"\nend tell",
            escape_applescript(dir),
            escape_applescript(command),
            escape_applescript(title)
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
