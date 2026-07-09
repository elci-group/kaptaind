pub fn handle_autostart() -> anyhow::Result<()> {
    crate::monitor::resume()
}
