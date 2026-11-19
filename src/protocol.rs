//! Messages pushed to WebSocket clients, as JSON tagged by `type`.

use serde::Serialize;

use crate::detect::Track;

#[derive(Debug, Clone, Serialize)]
pub struct Band {
    pub start_mhz: f32,
    pub stop_mhz: f32,
    pub bins: usize,
    pub fps: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// First message on every connection.
    Hello {
        device: String,
        version: String,
        band: Band,
    },
    /// One processed sweep.
    Frame {
        seq: u64,
        t_ms: u64,
        noise_floor_dbm: f32,
        threshold_dbm: f32,
        /// Power per bin, rounded to 0.1 dB to keep frames small.
        power_dbm: Vec<f32>,
        tracks: Vec<Track>,
    },
}

pub fn round_tenth(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_tagged() {
        let msg = ServerMsg::Hello {
            device: "dev".into(),
            version: "0.1.0".into(),
            band: Band {
                start_mhz: 2400.0,
                stop_mhz: 2483.5,
                bins: 256,
                fps: 10,
            },
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "hello");
        assert_eq!(json["band"]["bins"], 256);
    }

    #[test]
    fn rounding_keeps_one_decimal() {
        assert_eq!(round_tenth(-95.3449), -95.3);
        assert_eq!(serde_json::to_string(&round_tenth(-61.26)).unwrap(), "-61.3");
    }
}
