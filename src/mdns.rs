//! Advertises the node on the LAN as `_rfwatch._tcp` so clients (the Android app)
//! can find it without knowing its IP address.

use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo};

pub const SERVICE_TYPE: &str = "_rfwatch._tcp.local.";

/// Registers the service. Keep the returned daemon alive for as long as it should be visible.
pub fn advertise(instance: &str, port: u16) -> Result<ServiceDaemon> {
    let daemon = ServiceDaemon::new()?;
    let host = format!("{instance}.local.");
    let props = [("version", env!("CARGO_PKG_VERSION")), ("ws", "/ws")];
    let info = ServiceInfo::new(SERVICE_TYPE, instance, &host, "", port, &props[..])?.enable_addr_auto();
    daemon.register(info)?;
    Ok(daemon)
}
