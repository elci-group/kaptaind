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
/// Returns Ok(()) for benign commands, Err for suspicious patterns.
/// Non-breaking by default — callers should log warnings unless strict mode is on.
pub fn validate_shell_command(cmd: &str) -> Result<(), ShellValidationError> {
    // Reject command substitution
    if cmd.contains("$(") || cmd.contains('`') {
        return Err(ShellValidationError {
            reason: "command substitution detected".to_string(),
        });
    }
    // Reject dangerous redirections to system paths
    let dangerous_redirects = ["> /etc", "> /usr", "> /bin", "> /sbin", "> /lib", "< /etc", "< /usr", "< /bin", "< /sbin", "< /lib"];
    for pat in &dangerous_redirects {
        if cmd.contains(pat) {
            return Err(ShellValidationError {
                reason: format!("dangerous redirection pattern: {}", pat),
            });
        }
    }
    // Reject rm -rf /
    if cmd.contains("rm -rf /") || cmd.contains("rm -rf /*") {
        return Err(ShellValidationError {
            reason: "dangerous deletion pattern detected".to_string(),
        });
    }
    Ok(())
}
