//! neum: ordinal, reversed and byte-sum gematria over a stateful redb memory, with lists
//! and notes. The window lives in `main.rs`; everything it stands on is here, so it can be
//! tested without one.

pub mod gematria;
pub mod settings;
pub mod store;

pub use gematria::Mode;
pub use settings::Settings;
pub use store::{Found, Memory, Stats};

/// a byte count the way people read them: `0 B`, `12.3 KiB`, `4.0 MiB`
pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}
