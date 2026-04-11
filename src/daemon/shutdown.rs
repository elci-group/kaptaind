use tokio::sync::watch;

/// Broadcasts a shutdown signal to all tasks holding a ShutdownToken.
pub struct ShutdownHandle(watch::Sender<bool>);

/// Receives shutdown signals. Tasks wait on this to gracefully exit.
pub struct ShutdownToken(watch::Receiver<bool>);

/// Creates a shutdown signal channel.
pub fn channel() -> (ShutdownHandle, ShutdownToken) {
    let (tx, rx) = watch::channel(false);
    (ShutdownHandle(tx), ShutdownToken(rx))
}

impl ShutdownHandle {
    /// Signal all tasks to begin shutdown.
    pub fn signal(&self) {
        let _ = self.0.send(true);
    }
}

impl ShutdownToken {
    /// Clone the token for use in a different task.
    pub fn clone_token(&self) -> Self {
        ShutdownToken(self.0.clone())
    }

    /// Check if shutdown has been signaled (non-blocking).
    pub fn is_shutdown(&self) -> bool {
        *self.0.borrow()
    }

    /// Wait for the shutdown signal to be sent.
    pub async fn wait(&mut self) {
        let _ = self.0.wait_for(|v| *v).await;
    }
}
