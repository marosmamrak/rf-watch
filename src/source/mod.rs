//! Where sweeps come from. The detector and the server only see [`SweepSource`], so a
//! real front end (an SDR driver, a recorded capture) plugs in without touching them.

pub mod csv;
pub mod sim;

use anyhow::Result;

use crate::spectrum::{Band, Sweep};

pub trait SweepSource: Send {
    /// The band every sweep covers: `power_dbm.len() == band().bins`.
    fn band(&self) -> Band;

    /// The next sweep, `Ok(None)` once the source is exhausted (end of a recording).
    fn next_sweep(&mut self) -> Result<Option<Sweep>>;
}

impl<S: SweepSource + ?Sized> SweepSource for Box<S> {
    fn band(&self) -> Band {
        (**self).band()
    }

    fn next_sweep(&mut self) -> Result<Option<Sweep>> {
        (**self).next_sweep()
    }
}
