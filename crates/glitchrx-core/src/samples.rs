//! Reading raster bytes as audio samples, the way Audacity's
//! "Import Raw Data" does, and writing them back.
//!
//! An encoding says how many bytes make one sample and how those bytes map
//! to a level. Samples are `f64` in roughly -1.0..1.0, like Audacity's
//! internal float format. Writing back clips to the encoding's range, which
//! is where the bright, saturated databending look comes from.
//!
//! Only basic IEEE operations are used (SPEC D-25), so the same bytes come
//! out on every machine.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::{Error, Result};

/// A sample encoding from Audacity's Import Raw Data dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encoding {
    /// Unsigned 8-bit PCM: byte 128 is silence.
    #[default]
    U8,
    /// Signed 8-bit PCM: byte 0 is silence, 128 is the most negative.
    S8,
    S16Le,
    S16Be,
    S24Le,
    S24Be,
    /// G.711 mu-law: one byte along a logarithmic curve.
    ULaw,
    /// G.711 A-law: like mu-law with a slightly different curve.
    ALaw,
}

impl Encoding {
    pub const ALL: [Encoding; 8] = [
        Encoding::U8,
        Encoding::S8,
        Encoding::S16Le,
        Encoding::S16Be,
        Encoding::S24Le,
        Encoding::S24Be,
        Encoding::ULaw,
        Encoding::ALaw,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Encoding::U8 => "u8",
            Encoding::S8 => "s8",
            Encoding::S16Le => "s16le",
            Encoding::S16Be => "s16be",
            Encoding::S24Le => "s24le",
            Encoding::S24Be => "s24be",
            Encoding::ULaw => "ulaw",
            Encoding::ALaw => "alaw",
        }
    }

    /// One-line description for help text.
    pub fn describe(self) -> &'static str {
        match self {
            Encoding::U8 => "unsigned 8-bit: clean ghosts, colors rotate",
            Encoding::S8 => "signed 8-bit: clips at mid-brightness, inverts highlights",
            Encoding::S16Le | Encoding::S16Be => "signed 16-bit: ghosts under rainbow grain",
            Encoding::S24Le | Encoding::S24Be => "signed 24-bit: one sample per pixel",
            Encoding::ULaw | Encoding::ALaw => "companded 8-bit: posterized neon blocks",
        }
    }

    pub fn parse(s: &str) -> Result<Encoding> {
        Encoding::ALL
            .into_iter()
            .find(|e| e.name() == s.trim())
            .ok_or_else(|| {
                let names: Vec<_> = Encoding::ALL.iter().map(|e| e.name()).collect();
                Error::Recipe(format!(
                    "unknown encoding `{s}` (valid: {})",
                    names.join(", ")
                ))
            })
    }

    /// Bytes per sample.
    pub fn width(self) -> usize {
        match self {
            Encoding::U8 | Encoding::S8 | Encoding::ULaw | Encoding::ALaw => 1,
            Encoding::S16Le | Encoding::S16Be => 2,
            Encoding::S24Le | Encoding::S24Be => 3,
        }
    }

    /// Decode one sample from `b` (exactly `width()` bytes).
    #[inline]
    pub fn read(self, b: &[u8]) -> f64 {
        match self {
            Encoding::U8 => (b[0] as f64 - 128.0) / 128.0,
            Encoding::S8 => (b[0] as i8) as f64 / 128.0,
            Encoding::S16Le => i16::from_le_bytes([b[0], b[1]]) as f64 / 32768.0,
            Encoding::S16Be => i16::from_be_bytes([b[0], b[1]]) as f64 / 32768.0,
            Encoding::S24Le => i24(b[2], b[1], b[0]) as f64 / 8_388_608.0,
            Encoding::S24Be => i24(b[0], b[1], b[2]) as f64 / 8_388_608.0,
            Encoding::ULaw => ULAW_DECODE[b[0] as usize] as f64 / 32768.0,
            Encoding::ALaw => ALAW_DECODE[b[0] as usize] as f64 / 32768.0,
        }
    }

    /// Clip `x` to this encoding's range and write it into `b`.
    #[inline]
    pub fn write(self, x: f64, b: &mut [u8]) {
        match self {
            Encoding::U8 => b[0] = (quantize(x, 128.0, -128.0, 127.0) + 128.0) as u8,
            Encoding::S8 => b[0] = quantize(x, 128.0, -128.0, 127.0) as i8 as u8,
            Encoding::S16Le => {
                b.copy_from_slice(&(quantize(x, 32768.0, -32768.0, 32767.0) as i16).to_le_bytes())
            }
            Encoding::S16Be => {
                b.copy_from_slice(&(quantize(x, 32768.0, -32768.0, 32767.0) as i16).to_be_bytes())
            }
            Encoding::S24Le | Encoding::S24Be => {
                let v = quantize(x, 8_388_608.0, -8_388_608.0, 8_388_607.0) as i32;
                let [_, hi, mid, lo] = v.to_be_bytes();
                if self == Encoding::S24Le {
                    b.copy_from_slice(&[lo, mid, hi]);
                } else {
                    b.copy_from_slice(&[hi, mid, lo]);
                }
            }
            Encoding::ULaw => b[0] = ulaw_encode(quantize(x, 32768.0, -32768.0, 32767.0) as i32),
            Encoding::ALaw => b[0] = alaw_encode(quantize(x, 32768.0, -32768.0, 32767.0) as i32),
        }
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl Serialize for Encoding {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Encoding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Encoding::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Sign-extend a big-endian 24-bit value.
#[inline]
fn i24(hi: u8, mid: u8, lo: u8) -> i32 {
    i32::from_be_bytes([hi, mid, lo, 0]) >> 8
}

/// Scale, round to nearest and clamp. NaN (which no effect should produce)
/// becomes silence rather than an arbitrary value.
#[inline]
fn quantize(x: f64, scale: f64, lo: f64, hi: f64) -> f64 {
    let v = (x * scale).round();
    if v.is_nan() { 0.0 } else { v.clamp(lo, hi) }
}

// ---------------------------------------------------------------------------
// G.711. The decode tables are built at compile time from the standard's
// integer formulas; encoding follows the reference segment search.

const ULAW_BIAS: i32 = 0x84;
const ULAW_CLIP: i32 = 32635;

const fn ulaw_decode_one(byte: u8) -> i16 {
    let b = !byte;
    let exponent = ((b >> 4) & 0x07) as i32;
    let mantissa = (b & 0x0F) as i32;
    let magnitude = (((mantissa << 3) + ULAW_BIAS) << exponent) - ULAW_BIAS;
    if b & 0x80 != 0 {
        // Mu-law has a "negative zero" (0x7F). Decoding it to -1 instead of
        // 0 keeps bytes -> samples -> bytes exact: -1 encodes back to 0x7F.
        if magnitude == 0 {
            -1
        } else {
            -magnitude as i16
        }
    } else {
        magnitude as i16
    }
}

const fn alaw_decode_one(byte: u8) -> i16 {
    let a = byte ^ 0x55;
    let exponent = ((a >> 4) & 0x07) as i32;
    let mantissa = (a & 0x0F) as i32;
    let magnitude = if exponent == 0 {
        (mantissa << 4) + 8
    } else {
        ((mantissa << 4) + 0x108) << (exponent - 1)
    };
    if a & 0x80 != 0 {
        magnitude as i16
    } else {
        -magnitude as i16
    }
}

const fn table(f: fn_decode) -> [i16; 256] {
    let mut t = [0i16; 256];
    let mut i = 0;
    while i < 256 {
        t[i] = match f {
            fn_decode::ULaw => ulaw_decode_one(i as u8),
            fn_decode::ALaw => alaw_decode_one(i as u8),
        };
        i += 1;
    }
    t
}

// `const fn` pointers are not allowed in const evaluation, so pick by tag.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
enum fn_decode {
    ULaw,
    ALaw,
}

static ULAW_DECODE: [i16; 256] = table(fn_decode::ULaw);
static ALAW_DECODE: [i16; 256] = table(fn_decode::ALaw);

/// Index of the highest set bit of a value known to be below 2^15.
#[inline]
fn segment(v: i32, base_shift: u32) -> i32 {
    let mut seg = 0;
    while seg < 7 && (v >> (seg as u32 + base_shift + 1)) != 0 {
        seg += 1;
    }
    seg
}

fn ulaw_encode(sample: i32) -> u8 {
    let sign = if sample < 0 { 0x80 } else { 0 };
    let mag = sample.unsigned_abs().min(ULAW_CLIP as u32) as i32 + ULAW_BIAS;
    let exponent = segment(mag, 7);
    let mantissa = (mag >> (exponent + 3)) & 0x0F;
    !((sign | (exponent << 4) | mantissa) as u8)
}

fn alaw_encode(sample: i32) -> u8 {
    let sign = if sample >= 0 { 0x80 } else { 0 };
    let mag = sample.unsigned_abs().min(32767) as i32;
    let out = if mag < 256 {
        mag >> 4
    } else {
        let exponent = segment(mag, 7);
        (exponent << 4) | ((mag >> (exponent + 3)) & 0x0F)
    };
    ((sign | out) as u8) ^ 0x55
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No effect applied: bytes -> sample -> bytes is the identity for every
    /// encoding and every byte value (all 2^24 values for 24-bit would be
    /// slow, so a stride covers it).
    #[test]
    fn bytes_round_trip_exactly() {
        for enc in Encoding::ALL {
            let w = enc.width();
            let mut cases: Vec<Vec<u8>> = Vec::new();
            match w {
                1 => cases.extend((0..=255u8).map(|b| vec![b])),
                2 => cases.extend((0..=u16::MAX).map(|v| v.to_le_bytes().to_vec())),
                _ => cases.extend(
                    (0..(1u32 << 24))
                        .step_by(97)
                        .map(|v| v.to_le_bytes()[..3].to_vec()),
                ),
            }
            let mut out = vec![0u8; w];
            for c in cases {
                enc.write(enc.read(&c), &mut out);
                assert_eq!(out, c, "{enc}: {c:?}");
            }
        }
    }

    #[test]
    fn clipping_at_the_edges() {
        let mut b = [0u8; 3];
        Encoding::U8.write(5.0, &mut b[..1]);
        assert_eq!(b[0], 255);
        Encoding::U8.write(-5.0, &mut b[..1]);
        assert_eq!(b[0], 0);
        Encoding::U8.write(f64::NAN, &mut b[..1]);
        assert_eq!(b[0], 128);
        Encoding::S16Be.write(2.0, &mut b[..2]);
        assert_eq!(&b[..2], &[0x7F, 0xFF]);
        Encoding::S24Le.write(-2.0, &mut b);
        assert_eq!(b, [0x00, 0x00, 0x80]);
    }

    #[test]
    fn byte_order_and_signs() {
        assert_eq!(Encoding::S16Le.read(&[0x00, 0x40]), 0.5);
        assert_eq!(Encoding::S16Be.read(&[0x40, 0x00]), 0.5);
        assert_eq!(Encoding::S24Be.read(&[0xC0, 0, 0]), -0.5);
        assert_eq!(Encoding::U8.read(&[128]), 0.0);
        assert_eq!(Encoding::S8.read(&[0]), 0.0);
        assert_eq!(Encoding::S8.read(&[0x80]), -1.0);
    }

    /// Spot values from the G.711 tables.
    #[test]
    fn g711_reference_values() {
        assert_eq!(ULAW_DECODE[0xFF], 0);
        assert_eq!(ULAW_DECODE[0x00], -32124);
        assert_eq!(ULAW_DECODE[0x80], 32124);
        assert_eq!(ALAW_DECODE[0xD5], 8);
        assert_eq!(ALAW_DECODE[0x55], -8);
        assert_eq!(ALAW_DECODE[0xAA], 32256);
        assert_eq!(ulaw_encode(0), 0xFF);
        assert_eq!(alaw_encode(0), 0xD5);
    }

    #[test]
    fn names_parse() {
        for e in Encoding::ALL {
            assert_eq!(Encoding::parse(e.name()).unwrap(), e);
        }
        assert!(Encoding::parse("mp3").is_err());
    }
}
