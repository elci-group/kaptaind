#[cfg(feature = "experimental")]
pub fn gated() {}

#[cfg(all(unix, feature = "net"))]
pub async fn net_fetch() {}

pub fn always() {}
