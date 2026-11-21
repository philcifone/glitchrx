//! JPEG container: find the entropy-coded scan data, expose it as one
//! contiguous payload, and reassemble the file afterwards.
//!
//! # How the file is split
//!
//! A JPEG is a sequence of marker segments (`FF xx` plus, for most markers, a
//! two-byte length). Scan data follows each SOS segment and runs until the
//! next marker. Inside scan data a literal `0xFF` byte is always "stuffed" as
//! `FF 00`, and restart markers `FF D0`..`FF D7` may appear every N MCUs.
//!
//! The parser records each stretch of scan data between markers as a *run*.
//! Everything else (headers, tables, restart markers, fill bytes, thumbnails,
//! anything after the end-of-image marker) is *structure* and is copied
//! verbatim on output. That is the header-safe guarantee.
//!
//! In safe mode the runs are de-stuffed into the payload, and re-stuffed when
//! the file is reassembled, so no effect can ever forge a marker. In raw mode
//! the runs are exposed exactly as stored, as Audacity would see them.

use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// How effects see the scan data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// De-stuffed payload; output is always structurally valid.
    #[default]
    Safe,
    /// Bytes exactly as stored; effects can forge markers and truncate the image.
    Raw,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Safe => "safe",
            Mode::Raw => "raw",
        }
    }
}

/// One stretch of entropy-coded data.
#[derive(Debug, Clone)]
struct Run {
    /// Where the run sits in the original file (stuffed bytes).
    src: Range<usize>,
    /// Where its bytes sit in the payload.
    payload: Range<usize>,
}

/// Frame (SOF) details, for `--info` and sanity checks.
#[derive(Debug, Clone)]
pub struct Frame {
    pub sof: u8,
    pub width: u16,
    pub height: u16,
    /// `(component id, horizontal sampling, vertical sampling)`.
    pub components: Vec<(u8, u8, u8)>,
}

impl Frame {
    pub fn progressive(&self) -> bool {
        self.sof == 0xC2
    }

    /// Human-readable chroma subsampling.
    pub fn subsampling(&self) -> String {
        match self.components.as_slice() {
            [_] => "grayscale".into(),
            [(_, yh, yv), (_, 1, 1), (_, 1, 1)] => match (yh, yv) {
                (1, 1) => "4:4:4".into(),
                (2, 1) => "4:2:2".into(),
                (2, 2) => "4:2:0".into(),
                (1, 2) => "4:4:0".into(),
                (4, 1) => "4:1:1".into(),
                _ => format!("{yh}x{yv}"),
            },
            [_, _, _, _] => "4 components (CMYK/YCCK)".into(),
            other => format!("{} components", other.len()),
        }
    }
}

/// Facts about the file's layout, gathered while parsing.
#[derive(Debug, Clone)]
pub struct Info {
    pub frame: Frame,
    pub scans: u32,
    pub restart_interval: u16,
    pub restart_markers: u32,
    pub payload_len: usize,
    pub file_len: usize,
    pub has_exif_thumbnail: bool,
    /// Bytes after the primary image's EOI (gain maps, MPF images, ...).
    pub trailing_len: usize,
    /// No EOI was found (truncated, or glitched in raw mode).
    pub missing_eoi: bool,
}

/// A parsed JPEG whose scan payload can be edited and reassembled.
#[derive(Debug, Clone)]
pub struct Jpeg {
    original: Vec<u8>,
    runs: Vec<Run>,
    payload: Vec<u8>,
    mode: Mode,
    info: Info,
}

/// Quick check for the SOI marker.
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xD8])
}

impl Jpeg {
    /// Parse `bytes` and extract the payload.
    pub fn parse(bytes: Vec<u8>, mode: Mode) -> Result<Jpeg> {
        let layout = scan_layout(&bytes)?;
        let mut payload = Vec::with_capacity(bytes.len());
        let mut runs = Vec::with_capacity(layout.runs.len());
        for src in layout.runs {
            let start = payload.len();
            match mode {
                Mode::Safe => destuff_into(&bytes[src.clone()], &mut payload),
                Mode::Raw => payload.extend_from_slice(&bytes[src.clone()]),
            }
            runs.push(Run {
                src,
                payload: start..payload.len(),
            });
        }
        let info = Info {
            payload_len: payload.len(),
            file_len: bytes.len(),
            ..layout.info
        };
        Ok(Jpeg {
            original: bytes,
            runs,
            payload,
            mode,
            info,
        })
    }

    pub fn info(&self) -> &Info {
        &self.info
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The bytes effects operate on.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Mutable payload. Its length is fixed: effects must preserve length.
    pub fn payload_mut(&mut self) -> &mut [u8] {
        &mut self.payload
    }

    /// Reassemble the file: structure copied verbatim, runs re-stuffed
    /// (safe mode) or copied as edited (raw mode).
    pub fn finish(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.original.len() + self.original.len() / 64);
        let mut cursor = 0;
        for run in &self.runs {
            out.extend_from_slice(&self.original[cursor..run.src.start]);
            let data = &self.payload[run.payload.clone()];
            match self.mode {
                Mode::Safe => stuff_into(data, &mut out),
                Mode::Raw => out.extend_from_slice(data),
            }
            cursor = run.src.end;
        }
        out.extend_from_slice(&self.original[cursor..]);
        out
    }

    /// The structure (non-run) pieces of the original file, in order.
    /// Used by tests to prove the header-safe guarantee.
    pub fn structure(&self) -> Vec<&[u8]> {
        let mut parts = Vec::with_capacity(self.runs.len() + 1);
        let mut cursor = 0;
        for run in &self.runs {
            parts.push(&self.original[cursor..run.src.start]);
            cursor = run.src.end;
        }
        parts.push(&self.original[cursor..]);
        parts
    }

    /// Byte ranges of the runs in the original file (for tests).
    pub fn run_ranges(&self) -> Vec<Range<usize>> {
        self.runs.iter().map(|r| r.src.clone()).collect()
    }
}

/// Remove byte stuffing: `FF 00` becomes `FF`.
fn destuff_into(src: &[u8], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < src.len() {
        let b = src[i];
        out.push(b);
        // Runs only ever contain FF as part of an FF 00 pair (the layout
        // scanner ends a run at any other FF), so skip the 00.
        i += if b == 0xFF { 2 } else { 1 };
    }
}

/// Add byte stuffing: every `FF` is followed by `00`.
fn stuff_into(src: &[u8], out: &mut Vec<u8>) {
    for &b in src {
        out.push(b);
        if b == 0xFF {
            out.push(0x00);
        }
    }
}

struct Layout {
    runs: Vec<Range<usize>>,
    info: Info,
}

/// True for SOF markers whose coding we do not support.
fn unsupported_sof(m: u8) -> Option<&'static str> {
    match m {
        0xC3 => Some("lossless JPEG (SOF3)"),
        0xC5..=0xC7 => Some("hierarchical JPEG"),
        0xC9..=0xCB => Some("arithmetic-coded JPEG"),
        0xCD..=0xCF => Some("hierarchical arithmetic-coded JPEG"),
        _ => None,
    }
}

/// Walk the marker structure and locate every run of scan data.
fn scan_layout(b: &[u8]) -> Result<Layout> {
    if !sniff(b) {
        return Err(Error::NotJpeg);
    }
    let len = b.len();
    let mut pos = 2;
    let mut runs = Vec::new();
    let mut frame: Option<Frame> = None;
    let mut scans = 0u32;
    let mut restart_interval = 0u16;
    let mut restart_markers = 0u32;
    let mut has_exif_thumbnail = false;
    let mut eoi_end: Option<usize> = None;

    // Once image data has been seen, structural surprises are tolerated: the
    // rest of the file is treated as trailing data. This keeps files glitched
    // in raw mode (which may contain forged markers) usable as input.
    macro_rules! bail_or_stop {
        ($err:expr) => {
            if scans > 0 {
                break;
            } else {
                return Err($err);
            }
        };
    }

    while pos < len {
        if b[pos] != 0xFF {
            bail_or_stop!(Error::Malformed(format!("expected a marker at byte {pos}")));
        }
        // Skip fill bytes: any number of extra FFs before the marker code.
        while pos + 1 < len && b[pos + 1] == 0xFF {
            pos += 1;
        }
        if pos + 1 >= len {
            bail_or_stop!(Error::Malformed("file ends inside a marker".into()));
        }
        let marker = b[pos + 1];
        match marker {
            0xD9 => {
                eoi_end = Some(pos + 2);
                break;
            }
            // Standalone markers without a length (stray SOI, TEM, RST).
            0xD8 | 0x01 | 0xD0..=0xD7 => {
                pos += 2;
                continue;
            }
            _ => {}
        }
        if pos + 4 > len {
            bail_or_stop!(Error::Malformed("truncated segment header".into()));
        }
        let seg_len = u16::from_be_bytes([b[pos + 2], b[pos + 3]]) as usize;
        let seg_end = pos + 2 + seg_len;
        if seg_len < 2 || seg_end > len {
            bail_or_stop!(Error::Malformed(format!(
                "segment FF{marker:02X} at byte {pos} has a bad length"
            )));
        }
        let body = &b[pos + 4..seg_end];
        match marker {
            0xC0..=0xC2 => {
                if body.len() < 6 {
                    bail_or_stop!(Error::Malformed("short SOF segment".into()));
                }
                let n = body[5] as usize;
                if body.len() < 6 + 3 * n {
                    bail_or_stop!(Error::Malformed("short SOF component list".into()));
                }
                let components = (0..n)
                    .map(|i| {
                        let c = &body[6 + 3 * i..9 + 3 * i];
                        (c[0], c[1] >> 4, c[1] & 0x0F)
                    })
                    .collect();
                frame = Some(Frame {
                    sof: marker,
                    height: u16::from_be_bytes([body[1], body[2]]),
                    width: u16::from_be_bytes([body[3], body[4]]),
                    components,
                });
            }
            0xCC => {
                return Err(Error::Unsupported("arithmetic-coded JPEG (DAC)".into()));
            }
            m if unsupported_sof(m).is_some() => {
                return Err(Error::Unsupported(unsupported_sof(m).unwrap().into()));
            }
            0xDD if body.len() >= 2 => {
                restart_interval = u16::from_be_bytes([body[0], body[1]]);
            }
            0xE1 => {
                // An EXIF block that contains an SOI is carrying a thumbnail.
                if body.starts_with(b"Exif\0\0") && body.windows(3).any(|w| w == [0xFF, 0xD8, 0xFF])
                {
                    has_exif_thumbnail = true;
                }
            }
            0xDA => {
                if frame.is_none() {
                    return Err(Error::Malformed(
                        "scan (SOS) before frame header (SOF)".into(),
                    ));
                }
                scans += 1;
                pos = seg_end;
                // Scan data: collect runs, split at restart markers, until
                // any other marker (or a fill FF) is reached.
                let mut run_start = pos;
                let mut i = pos;
                while i < len {
                    if b[i] == 0xFF {
                        if i + 1 >= len {
                            break;
                        }
                        let next = b[i + 1];
                        if next == 0x00 {
                            i += 2;
                            continue;
                        }
                        if (0xD0..=0xD7).contains(&next) {
                            runs.push(run_start..i);
                            restart_markers += 1;
                            i += 2;
                            run_start = i;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                runs.push(run_start..i);
                pos = i;
                continue;
            }
            _ => {}
        }
        pos = seg_end;
    }

    let frame = frame.ok_or_else(|| Error::Malformed("no frame header (SOF) found".into()))?;
    if scans == 0 {
        return Err(Error::Malformed("no image data (SOS) found".into()));
    }
    Ok(Layout {
        runs,
        info: Info {
            frame,
            scans,
            restart_interval,
            restart_markers,
            payload_len: 0,
            file_len: len,
            has_exif_thumbnail,
            trailing_len: eoi_end.map_or(0, |e| len - e),
            missing_eoi: eoi_end.is_none(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny hand-built JPEG skeleton: SOI, SOF0, SOS, scan data, EOI.
    /// Not decodable (no tables) but structurally valid for the parser.
    fn skeleton(scan: &[u8]) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8];
        // SOF0: len 11, precision 8, 16x16, 1 component (id 1, 1x1, table 0)
        v.extend_from_slice(&[0xFF, 0xC0, 0, 11, 8, 0, 16, 0, 16, 1, 1, 0x11, 0]);
        // SOS: len 8, 1 component, Ss Se Ah/Al
        v.extend_from_slice(&[0xFF, 0xDA, 0, 8, 1, 1, 0x00, 0, 63, 0]);
        v.extend_from_slice(scan);
        v.extend_from_slice(&[0xFF, 0xD9]);
        v
    }

    #[test]
    fn destuff_and_restuff() {
        let file = skeleton(&[1, 0xFF, 0x00, 2, 0xFF, 0xD0, 3, 0xFF, 0x00]);
        let j = Jpeg::parse(file.clone(), Mode::Safe).unwrap();
        assert_eq!(j.payload(), &[1, 0xFF, 2, 3, 0xFF]);
        assert_eq!(j.info().restart_markers, 1);
        assert_eq!(j.finish(), file);
    }

    #[test]
    fn restuffs_new_ff_bytes() {
        let file = skeleton(&[1, 2, 3]);
        let mut j = Jpeg::parse(file, Mode::Safe).unwrap();
        j.payload_mut()[1] = 0xFF;
        let out = j.finish();
        let re = Jpeg::parse(out.clone(), Mode::Safe).unwrap();
        assert_eq!(re.payload(), &[1, 0xFF, 3]);
        assert!(out.windows(4).any(|w| w == [1, 0xFF, 0x00, 3]));
    }

    #[test]
    fn raw_mode_can_forge_markers() {
        let file = skeleton(&[1, 2, 3, 4]);
        let mut j = Jpeg::parse(file, Mode::Raw).unwrap();
        j.payload_mut()[1] = 0xFF;
        j.payload_mut()[2] = 0xD9; // forged EOI
        let out = j.finish();
        // Re-parsing stops at the forged EOI; the rest is trailing data.
        let re = Jpeg::parse(out, Mode::Safe).unwrap();
        assert_eq!(re.payload(), &[1]);
        assert!(re.info().trailing_len > 0);
    }

    #[test]
    fn fill_bytes_are_structure() {
        let mut file = skeleton(&[5, 6]);
        let n = file.len();
        file.splice(n - 2..n - 2, [0xFF, 0xFF]);
        let j = Jpeg::parse(file.clone(), Mode::Safe).unwrap();
        assert_eq!(j.payload(), &[5, 6]);
        assert_eq!(j.finish(), file);
    }

    #[test]
    fn rejects_non_jpeg_and_arithmetic() {
        assert!(matches!(
            Jpeg::parse(vec![0x89, b'P'], Mode::Safe),
            Err(Error::NotJpeg)
        ));
        let mut file = skeleton(&[1]);
        file[3] = 0xC9;
        assert!(matches!(
            Jpeg::parse(file, Mode::Safe),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn truncated_file_still_parses() {
        let mut file = skeleton(&[1, 2, 3, 4]);
        file.truncate(file.len() - 3); // cut EOI and one data byte
        let j = Jpeg::parse(file.clone(), Mode::Safe).unwrap();
        assert!(j.info().missing_eoi);
        assert_eq!(j.finish(), file);
    }
}
