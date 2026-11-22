//! The restart-interval setting: how tall a glitch band can grow.
//!
//! A JPEG restart marker resets the decoder, so damage stops at the next
//! marker. Adding markers every N MCU rows (see [`crate::turbo::set_restarts`])
//! turns "everything below the first hit is ruined" into separate bands.
//!
//! The interval can be given in MCU rows (exact, resolution-dependent) or as
//! a percent of the image height (the same look at any resolution):
//!
//! - `12`  a marker every 12 MCU rows
//! - `8%`  a marker every 8% of the image height, rounded to whole MCU rows
//! - `0`   remove all restart markers

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};
use crate::formats::jpeg::Frame;

/// A restart interval, before it is resolved against an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Restart {
    /// Every N MCU rows (`0` = none).
    Rows(u16),
    /// Every N/1000 percent of the image height (`8000` = 8%), at least one
    /// MCU row.
    MilliPercent(u32),
}

impl Restart {
    /// Parse `12`, `8%` or `2.5%`.
    pub fn parse(s: &str) -> Result<Restart> {
        let bad = || Error::Recipe(format!("restart: expected ROWS or PERCENT%, got `{s}`"));
        let s = s.trim();
        if let Some(p) = s.strip_suffix('%') {
            let v: f64 = p.trim().parse().map_err(|_| bad())?;
            if !(v > 0.0 && v <= 100.0) {
                return Err(Error::Recipe(format!(
                    "restart: percent must be in (0, 100], got {v}"
                )));
            }
            Ok(Restart::MilliPercent((v * 1000.0).round() as u32))
        } else {
            s.parse().map(Restart::Rows).map_err(|_| bad())
        }
    }

    /// The interval in MCU rows for this image.
    pub fn rows_for(self, frame: &Frame) -> u16 {
        match self {
            Restart::Rows(n) => n,
            Restart::MilliPercent(mp) => {
                let total = (frame.height as u32).div_ceil(frame.mcu_height());
                // Round to nearest, then keep within 1..=total.
                let rows = (total as u64 * mp as u64 + 50_000) / 100_000;
                rows.clamp(1, total.max(1) as u64) as u16
            }
        }
    }
}

impl fmt::Display for Restart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Restart::Rows(n) => write!(f, "{n}"),
            Restart::MilliPercent(mp) if mp % 1000 == 0 => write!(f, "{}%", mp / 1000),
            Restart::MilliPercent(mp) => write!(f, "{}%", *mp as f64 / 1000.0),
        }
    }
}

// In a recipe: rows as a TOML integer, percent as a string.
impl Serialize for Restart {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Restart::Rows(n) => s.serialize_u16(*n),
            Restart::MilliPercent(_) => s.serialize_str(&self.to_string()),
        }
    }
}

impl<'de> Deserialize<'de> for Restart {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Int(i64),
            Text(String),
        }
        match Raw::deserialize(d)? {
            Raw::Int(i) => u16::try_from(i)
                .map(Restart::Rows)
                .map_err(|_| serde::de::Error::custom(format!("restart: {i} is out of range"))),
            Raw::Text(t) => Restart::parse(&t).map_err(serde::de::Error::custom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(height: u16, v: u8) -> Frame {
        Frame {
            sof: 0xC0,
            width: 100,
            height,
            components: vec![(1, 2, v), (2, 1, 1), (3, 1, 1)],
        }
    }

    #[test]
    fn parse_and_display() {
        assert_eq!(Restart::parse("12").unwrap(), Restart::Rows(12));
        assert_eq!(Restart::parse("8%").unwrap(), Restart::MilliPercent(8000));
        assert_eq!(Restart::parse("2.5%").unwrap().to_string(), "2.5%");
        assert_eq!(Restart::parse("0").unwrap(), Restart::Rows(0));
        for bad in ["", "x", "-1", "0%", "101%", "70000"] {
            assert!(Restart::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn percent_resolves_against_mcu_rows() {
        // 4:2:0: 16 px MCUs, 3746 px tall -> 235 rows; 8% -> 19 rows.
        assert_eq!(Restart::MilliPercent(8000).rows_for(&frame(3746, 2)), 19);
        // 4:2:2: 8 px MCUs -> 469 rows; 8% -> 38.
        assert_eq!(Restart::MilliPercent(8000).rows_for(&frame(3746, 1)), 38);
        // Tiny images still get at least one row.
        assert_eq!(Restart::MilliPercent(100).rows_for(&frame(16, 2)), 1);
        assert_eq!(Restart::Rows(5).rows_for(&frame(16, 2)), 5);
    }
}
