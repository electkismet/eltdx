//! Pure MAC and MAC-EX value/codec groundwork for T25 Stage A.
//!
//! This module is intentionally not registered by the crate root. Stage B owns
//! public feature registration, owners, profiles, stream association and I/O.
pub mod bitmap;
pub mod commands;
pub mod models;
pub mod routing;

pub use bitmap::{FieldId, FieldSelection, PresetField};
pub use models::{MacCode, MacInstrumentId, MacMarketId, WireFloat};
