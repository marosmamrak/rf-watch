//! rf-watch: a passive RF drone-link detector demo.
//!
//! [`sim`] produces power-spectrum sweeps, [`detect`] finds emitters in them, [`server`]
//! streams the results over WebSocket and serves the embedded UI, [`mdns`] makes the node
//! discoverable on the LAN.

pub mod detect;
pub mod mdns;
pub mod protocol;
pub mod server;
pub mod sim;
