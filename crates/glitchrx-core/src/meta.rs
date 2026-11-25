//! Removing where a photo was taken from its metadata (SPEC 8.1, D-63).
//!
//! A project file carries the original photo, and a phone photo usually
//! says where it was taken. Before it goes into a project, the location is
//! removed the way a phone does when sharing: the rest of the metadata
//! (camera, date, orientation, color profile) stays.
//!
//! Everything is done **in place**, without moving a byte: values are
//! overwritten with zeros or spaces, never cut out (PNG text chunks, which
//! stand alone, may also be dropped whole; see [`png`]). The image data is not
//! touched, so the photo decodes to exactly the same raster and a recipe
//! replays to the same bytes (D-25). Where the location can hide:
//!
//! - **EXIF GPS**, a block of its own (the GPS IFD) inside the EXIF data:
//!   in a JPEG's APP1 segment, a PNG's `eXIf` chunk, or a TIFF file's own
//!   directory. Its entries and their values are zeroed and its entry
//!   count set to 0, which leaves a valid, empty block behind.
//! - **XMP** (Adobe's XML metadata): `exif:GPS...` properties and the
//!   city, state, country and location fields; their values become spaces.
//! - **IPTC** (the older press metadata, in Photoshop's APP13 block or a
//!   TIFF tag): city, sub-location, state, country and location name.
//!
//! A photo whose metadata cannot be followed is refused rather than passed
//! on with its location possibly intact.

use crate::codec::Format;
use crate::error::{Error, Result};

/// A copy of `bytes` (a JPEG, PNG or TIFF file) without its location.
pub fn remove_location(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut out = bytes.to_vec();
    match Format::sniff(bytes) {
        Some(Format::Jpeg) => jpeg(&mut out)?,
        Some(Format::Png) => return png(bytes),
        Some(Format::Tiff) => tiff_file(&mut out)?,
        _ => return Err(Error::UnknownFormat),
    }
    Ok(out)
}

fn refuse(what: &str) -> Error {
    Error::Malformed(format!("cannot remove the location: {what}"))
}

// ---------------------------------------------------------------------------
// JPEG: marker segments up to the first scan.

const XMP_ID: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
const XMP_EXT_ID: &[u8] = b"http://ns.adobe.com/xmp/extension/\0";
const PHOTOSHOP_ID: &[u8] = b"Photoshop 3.0\0";

fn jpeg(b: &mut [u8]) -> Result<()> {
    let mut i = 2; // after SOI
    loop {
        if i + 4 > b.len() || b[i] != 0xFF {
            return Ok(());
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1; // fill byte
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return Ok(()); // start of scan, end of image: no metadata after
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if len < 2 || i + 2 + len > b.len() {
            return Ok(());
        }
        let p = &mut b[i + 4..i + 2 + len];
        match marker {
            0xE1 if p.starts_with(b"Exif\0\0") => clear_gps(&mut p[6..])?,
            0xE1 if p.starts_with(XMP_ID) => blank_xmp(&mut p[XMP_ID.len()..]),
            0xE1 if p.starts_with(XMP_EXT_ID) => blank_xmp(&mut p[XMP_EXT_ID.len()..]),
            0xED if p.starts_with(PHOTOSHOP_ID) => blank_photoshop(&mut p[PHOTOSHOP_ID.len()..]),
            _ => {}
        }
        i += 2 + len;
    }
}

// ---------------------------------------------------------------------------
// TIFF structure: the EXIF block, and TIFF files themselves.

/// EXIF tags that point at metadata.
const GPS_IFD: u16 = 0x8825;
const XMP_TAG: u16 = 700;
const IPTC_TAG: u16 = 33723;

/// Reads a TIFF-structured buffer in its own byte order.
struct Tiff {
    le: bool,
}

impl Tiff {
    fn of(t: &[u8]) -> Option<Tiff> {
        match t.get(..4)? {
            b"II*\0" => Some(Tiff { le: true }),
            b"MM\0*" => Some(Tiff { le: false }),
            _ => None,
        }
    }

    fn u16(&self, t: &[u8], i: usize) -> Option<u16> {
        let b = t.get(i..i + 2)?;
        Some(if self.le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }

    fn u32(&self, t: &[u8], i: usize) -> Option<u32> {
        let b = t.get(i..i + 4)?;
        let a = [b[0], b[1], b[2], b[3]];
        Some(if self.le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    }

    fn put_u16(&self, t: &mut [u8], i: usize, v: u16) {
        let b = if self.le {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        };
        t[i..i + 2].copy_from_slice(&b);
    }

    /// The directory at `ifd`: its entry count, checked to fit.
    fn count(&self, t: &[u8], ifd: usize) -> Option<usize> {
        let n = self.u16(t, ifd)? as usize;
        (ifd + 2 + n * 12 + 4 <= t.len()).then_some(n)
    }

    /// Where an entry's value lives and how long it is: inside the entry
    /// when it fits in 4 bytes, else at the offset the entry holds.
    fn value(&self, t: &[u8], entry: usize) -> Option<(usize, usize)> {
        let kind = self.u16(t, entry + 2)?;
        let size = match kind {
            1 | 2 | 6 | 7 => 1,
            3 | 8 => 2,
            4 | 9 | 11 | 13 => 4,
            5 | 10 | 12 => 8,
            _ => return None,
        };
        let len = size * self.u32(t, entry + 4)? as usize;
        let at = if len <= 4 {
            entry + 8
        } else {
            self.u32(t, entry + 8)? as usize
        };
        (at.checked_add(len)? <= t.len()).then_some((at, len))
    }

    /// Each directory in the chain from the header: (offset, entry count).
    /// At most 64, so a loop in a broken file ends.
    fn chain(&self, t: &[u8]) -> Option<Vec<(usize, usize)>> {
        let mut out = Vec::new();
        let mut ifd = self.u32(t, 4)? as usize;
        while ifd != 0 && out.len() < 64 {
            let n = self.count(t, ifd)?;
            out.push((ifd, n));
            ifd = self.u32(t, ifd + 2 + n * 12)? as usize;
        }
        Some(out)
    }
}

/// Empty the GPS directory of a TIFF-structured buffer (an EXIF block or a
/// TIFF file), and blank the XMP and IPTC it holds as tags.
fn clear_tiff(t: &mut [u8]) -> Option<()> {
    let r = Tiff::of(t)?;
    for (ifd, n) in r.chain(t)? {
        for k in 0..n {
            let e = ifd + 2 + k * 12;
            match r.u16(t, e)? {
                GPS_IFD => {
                    let gps = r.u32(t, e + 8)? as usize;
                    clear_gps_ifd(&r, t, gps)?;
                }
                XMP_TAG => {
                    let (at, len) = r.value(t, e)?;
                    blank_xmp(&mut t[at..at + len]);
                }
                IPTC_TAG => {
                    let (at, len) = r.value(t, e)?;
                    blank_iim(&mut t[at..at + len]);
                }
                _ => {}
            }
        }
    }
    Some(())
}

/// Zero each GPS entry's value, then the entries, and leave 0 of them.
fn clear_gps_ifd(r: &Tiff, t: &mut [u8], gps: usize) -> Option<()> {
    let n = r.count(t, gps)?;
    for k in 0..n {
        let (at, len) = r.value(t, gps + 2 + k * 12)?;
        t[at..at + len].fill(0);
    }
    // The entries and the next-directory offset after them: with a count
    // of 0, a reader finds that offset at gps + 2, now zero (no next).
    t[gps + 2..gps + 2 + n * 12 + 4].fill(0);
    r.put_u16(t, gps, 0);
    Some(())
}

fn clear_gps(t: &mut [u8]) -> Result<()> {
    clear_tiff(t).ok_or_else(|| refuse("unreadable EXIF"))
}

fn tiff_file(b: &mut [u8]) -> Result<()> {
    clear_tiff(b).ok_or_else(|| refuse("unreadable TIFF directory"))
}

// ---------------------------------------------------------------------------
// PNG: chunks, each with a CRC that must be redone after a change.
//
// PNG is the one format where metadata is also *removed* rather than
// blanked: chunks stand alone, so dropping a text chunk moves nothing the
// decoder reads (and this decoder takes no orientation from PNG metadata).
// That covers what cannot be blanked in place: compressed XMP, and the
// "Raw profile type ..." chunks in which ImageMagick and exiftool keep
// whole EXIF, IPTC or XMP blocks as compressed hex.

/// Text chunks dropped whole, by keyword.
const PNG_DROPPED: &[&[u8]] = &[
    b"Raw profile type exif",
    b"Raw profile type APP1",
    b"Raw profile type iptc",
    b"Raw profile type 8bim",
    b"Raw profile type xmp",
];

/// A copy of the PNG `b` without its location.
fn png(b: &[u8]) -> Result<Vec<u8>> {
    let mut out = b[..8].to_vec(); // the signature
    let mut i = 8;
    while i + 12 <= b.len() {
        let len = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        let end = i + 8 + len;
        if end + 4 > b.len() {
            return Err(refuse("a PNG chunk runs past the end"));
        }
        let kind = [b[i + 4], b[i + 5], b[i + 6], b[i + 7]];
        let mut chunk = b[i..end + 4].to_vec();
        let data = 8..8 + len;
        let keyword = &b[i + 8..end][..b[i + 8..end].iter().position(|&c| c == 0).unwrap_or(0)];
        let text = matches!(&kind, b"tEXt" | b"zTXt" | b"iTXt");
        let keep = match &kind {
            _ if text && PNG_DROPPED.contains(&keyword) => false,
            b"eXIf" => {
                clear_gps(&mut chunk[data])?;
                true
            }
            b"iTXt" if keyword == b"XML:com.adobe.xmp" => xmp_itxt(&mut chunk[data]),
            _ => true,
        };
        if keep {
            let crc = crc32fast::hash(&chunk[4..8 + len]);
            chunk[8 + len..].copy_from_slice(&crc.to_be_bytes());
            out.extend_from_slice(&chunk);
        }
        i = end + 4;
        if &kind == b"IEND" {
            break;
        }
    }
    // Anything after IEND is not part of the picture; leave it out too.
    Ok(out)
}

/// An iTXt chunk holding XMP: keyword, NUL, compression flag and method,
/// language tag, NUL, translated keyword, NUL, then the text. Blanked in
/// place; false (drop the chunk) when it is compressed or malformed.
fn xmp_itxt(d: &mut [u8]) -> bool {
    const KEY: &[u8] = b"XML:com.adobe.xmp\0";
    let rest = KEY.len();
    if d.get(rest) != Some(&0) {
        return false; // compressed
    }
    let mut at = rest + 2;
    for _ in 0..2 {
        let Some(nul) = d.get(at..).and_then(|t| t.iter().position(|&c| c == 0)) else {
            return false;
        };
        at += nul + 1;
    }
    blank_xmp(&mut d[at..]);
    true
}

// ---------------------------------------------------------------------------
// XMP: XML text. Location properties keep their names; their values
// become spaces, which leaves well-formed XML of the same length.

const XMP_LOCATION: &[&[u8]] = &[
    b"exif:GPS",
    b"exifEX:GPS",
    b"photoshop:City",
    b"photoshop:State",
    b"photoshop:Country",
    b"Iptc4xmpCore:Location",
    b"Iptc4xmpCore:CountryCode",
    b"Iptc4xmpExt:LocationCreated",
    b"Iptc4xmpExt:LocationShown",
];

fn is_name(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b':')
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

fn blank_xmp(x: &mut [u8]) {
    for prop in XMP_LOCATION {
        let mut from = 0;
        while let Some(i) = find(x, prop, from) {
            let mut j = i + prop.len();
            while j < x.len() && is_name(x[j]) {
                j += 1;
            }
            from = j;
            let before = if i > 0 { x[i - 1] } else { b' ' };
            if before == b'<' {
                blank_element(x, i, j);
            } else if before != b'/' && before.is_ascii_whitespace() {
                blank_attribute(x, j);
            }
        }
    }
}

/// `name="value"` (or single quotes) starting after the name at `j`.
fn blank_attribute(x: &mut [u8], mut j: usize) {
    while j < x.len() && x[j].is_ascii_whitespace() {
        j += 1;
    }
    if x.get(j) != Some(&b'=') {
        return;
    }
    j += 1;
    while j < x.len() && x[j].is_ascii_whitespace() {
        j += 1;
    }
    let Some(&q) = x.get(j).filter(|&&q| q == b'"' || q == b'\'') else {
        return;
    };
    if let Some(end) = x[j + 1..].iter().position(|&c| c == q) {
        x[j + 1..j + 1 + end].fill(b' ');
    }
}

/// `<name ...>content</name>`: the content (text or nested elements) becomes
/// spaces. The name runs from `i` to `j`.
fn blank_element(x: &mut [u8], i: usize, j: usize) {
    let Some(gt) = x[j..].iter().position(|&c| c == b'>').map(|p| p + j) else {
        return;
    };
    if x[gt - 1] == b'/' {
        // Self-closing: any value is in its attributes.
        let mut k = j;
        while k < gt {
            if x[k] == b'=' {
                blank_attribute(x, k);
            }
            k += 1;
        }
        return;
    }
    let mut close = b"</".to_vec();
    close.extend_from_slice(&x[i..j]);
    if let Some(end) = find(x, &close, gt + 1) {
        x[gt + 1..end].fill(b' ');
    }
}

// ---------------------------------------------------------------------------
// IPTC: Photoshop image resources holding IIM datasets.

/// Photoshop's image resources ("8BIM" blocks); 0x0404 holds IPTC.
fn blank_photoshop(p: &mut [u8]) {
    let mut i = 0;
    while i + 12 <= p.len() && &p[i..i + 4] == b"8BIM" {
        let id = u16::from_be_bytes([p[i + 4], p[i + 5]]);
        // A Pascal-string name, padded so length byte and name are even.
        let name = p[i + 6] as usize;
        let at = i + 6 + ((name + 2) & !1);
        let Some(size) = p
            .get(at..at + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]) as usize)
        else {
            return;
        };
        let data = at + 4;
        if data + size > p.len() {
            return;
        }
        if id == 0x0404 {
            blank_iim(&mut p[data..data + size]);
        }
        i = data + ((size + 1) & !1);
    }
}

/// IIM datasets in record 2 that say where: content location code and
/// name, city, sub-location, state, country code and name.
const IIM_LOCATION: &[u8] = &[26, 27, 90, 92, 95, 100, 101];

fn blank_iim(d: &mut [u8]) {
    let mut i = 0;
    while i + 5 <= d.len() && d[i] == 0x1C {
        let (record, set) = (d[i + 1], d[i + 2]);
        let len = u16::from_be_bytes([d[i + 3], d[i + 4]]) as usize;
        if len & 0x8000 != 0 {
            return; // an extended length: never used for text fields
        }
        let at = i + 5;
        if at + len > d.len() {
            return;
        }
        if record == 2 && IIM_LOCATION.contains(&set) {
            d[at..at + len].fill(b' ');
        }
        i = at + len;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::codec::{self, Format};
    use crate::raster::Raster;

    /// An EXIF block: IFD0 with Orientation 6 and a GPS pointer; the GPS
    /// directory with a latitude reference (inline) and a latitude
    /// (three rationals, stored after the directory). The GPS values are
    /// marked 0xA5 so a test can look for them.
    pub(crate) fn exif(le: bool) -> Vec<u8> {
        let w16 = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        let w32 = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        let mut t = Vec::new();
        t.extend_from_slice(if le { b"II*\0" } else { b"MM\0*" });
        t.extend_from_slice(&w32(8));
        // IFD0 at 8: two entries.
        t.extend_from_slice(&w16(2));
        t.extend_from_slice(&w16(0x0112));
        t.extend_from_slice(&w16(3));
        t.extend_from_slice(&w32(1));
        t.extend_from_slice(&w16(6));
        t.extend_from_slice(&[0, 0]);
        let gps = 8 + 2 + 2 * 12 + 4;
        t.extend_from_slice(&w16(GPS_IFD));
        t.extend_from_slice(&w16(4));
        t.extend_from_slice(&w32(1));
        t.extend_from_slice(&w32(gps as u32));
        t.extend_from_slice(&w32(0));
        // GPS IFD: two entries.
        assert_eq!(t.len(), gps);
        let data = gps + 2 + 2 * 12 + 4;
        t.extend_from_slice(&w16(2));
        t.extend_from_slice(&w16(1)); // GPSLatitudeRef, ASCII "N\0"
        t.extend_from_slice(&w16(2));
        t.extend_from_slice(&w32(2));
        t.extend_from_slice(&[b'N', 0, 0, 0]);
        t.extend_from_slice(&w16(2)); // GPSLatitude, 3 RATIONAL
        t.extend_from_slice(&w16(5));
        t.extend_from_slice(&w32(3));
        t.extend_from_slice(&w32(data as u32));
        t.extend_from_slice(&w32(0));
        t.extend_from_slice(&[0xA5; 24]);
        t
    }

    pub(crate) fn xmp() -> Vec<u8> {
        br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF><rdf:Description exif:GPSLatitude="40,26.767N" exif:GPSLongitude = '79,58.933W' tiff:Make="Apple">
<photoshop:City>Springfield</photoshop:City><Iptc4xmpCore:Location><rdf:Alt><rdf:li>Home</rdf:li></rdf:Alt></Iptc4xmpCore:Location>
<exif:GPSAltitude rdf:value="300/1"/></rdf:Description></rdf:RDF></x:xmpmeta>"#
            .to_vec()
    }

    pub(crate) fn iptc() -> Vec<u8> {
        let mut iim = Vec::new();
        for (set, text) in [
            (90u8, &b"Springfield"[..]),
            (80, b"A. Photographer"),
            (101, b"USA"),
        ] {
            iim.extend_from_slice(&[0x1C, 2, set]);
            iim.extend_from_slice(&(text.len() as u16).to_be_bytes());
            iim.extend_from_slice(text);
        }
        let mut p = PHOTOSHOP_ID.to_vec();
        p.extend_from_slice(b"8BIM");
        p.extend_from_slice(&0x0404u16.to_be_bytes());
        p.extend_from_slice(&[0, 0]); // empty name, padded
        p.extend_from_slice(&(iim.len() as u32).to_be_bytes());
        p.extend_from_slice(&iim);
        if iim.len() % 2 == 1 {
            p.push(0);
        }
        p
    }

    fn sample() -> Raster {
        let bytes = (0..16 * 12)
            .flat_map(|i| [(i * 3) as u8, (i * 5) as u8, (i * 7) as u8])
            .collect();
        Raster::new(16, 12, bytes)
    }

    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut s = vec![0xFF, marker];
        s.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        s.extend_from_slice(payload);
        s
    }

    /// A JPEG with EXIF (GPS), XMP and IPTC location, in that order after SOI.
    pub(crate) fn located_jpeg(le: bool) -> Vec<u8> {
        let plain = codec::encode(&sample(), Format::Jpeg, None).unwrap();
        let mut ex = b"Exif\0\0".to_vec();
        ex.extend_from_slice(&exif(le));
        let mut x = XMP_ID.to_vec();
        x.extend_from_slice(&xmp());
        let mut out = plain[..2].to_vec();
        out.extend(segment(0xE1, &ex));
        out.extend(segment(0xE1, &x));
        out.extend(segment(0xED, &iptc()));
        out.extend_from_slice(&plain[2..]);
        out
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        find(hay, needle, 0).is_some()
    }

    fn assert_no_location(b: &[u8]) {
        for gone in [
            &b"Springfield"[..],
            b"40,26.767N",
            b"79,58.933W",
            b"Home",
            b"USA",
            b"300/1",
            &[0xA5; 24],
        ] {
            assert!(
                !contains(b, gone),
                "{:?} is still there",
                String::from_utf8_lossy(gone)
            );
        }
    }

    #[test]
    fn jpeg_location_goes_and_the_picture_stays() {
        for le in [true, false] {
            let src = located_jpeg(le);
            let out = remove_location(&src).unwrap();
            assert_eq!(out.len(), src.len(), "nothing moves");
            assert_no_location(&out);
            // What is not location stays: the camera, the person, orientation.
            assert!(contains(&out, b"tiff:Make=\"Apple\""));
            assert!(contains(&out, b"A. Photographer"));
            assert!(
                contains(&out, b"exif:GPSLatitude=\""),
                "the names stay, as valid XML"
            );
            assert_eq!(codec::jpeg_orientation(&out), 6);
            let (a, b) = (codec::decode(&src).unwrap(), codec::decode(&out).unwrap());
            assert_eq!(a.raster, b.raster);
            assert_eq!(a.orientation, b.orientation);
            // The block that is left is an empty GPS directory: removing
            // again changes nothing.
            assert_eq!(remove_location(&out).unwrap(), out);
        }
    }

    #[test]
    fn png_exif_and_xmp_lose_their_location_with_good_crcs() {
        let plain = codec::encode(&sample(), Format::Png, None).unwrap();
        let chunk = |kind: &[u8], data: &[u8]| {
            let mut c = (data.len() as u32).to_be_bytes().to_vec();
            c.extend_from_slice(kind);
            c.extend_from_slice(data);
            let crc = crc32fast::hash(&c[4..]);
            c.extend_from_slice(&crc.to_be_bytes());
            c
        };
        let mut itxt = b"XML:com.adobe.xmp\0\0\0\0\0".to_vec();
        itxt.extend_from_slice(&xmp());
        // After the signature and IHDR (8 + 25 bytes).
        let mut src = plain[..33].to_vec();
        src.extend(chunk(b"eXIf", &exif(false)));
        src.extend(chunk(b"iTXt", &itxt));
        // Whole metadata blocks as compressed hex (ImageMagick, exiftool),
        // compressed XMP, and a comment that has nothing to do with it.
        src.extend(chunk(b"zTXt", b"Raw profile type iptc\0\0x\x9c\x03\x00"));
        src.extend(chunk(
            b"iTXt",
            b"XML:com.adobe.xmp\0\x01\0\0\0x\x9c\x03\x00",
        ));
        src.extend(chunk(b"tEXt", b"Comment\0made on a Tuesday"));
        src.extend_from_slice(&plain[33..]);
        let out = remove_location(&src).unwrap();
        assert_no_location(&out);
        assert!(!contains(&out, b"Raw profile type iptc"));
        assert_eq!(
            out.windows(17)
                .filter(|w| w == b"XML:com.adobe.xmp")
                .count(),
            1
        );
        assert!(contains(&out, b"made on a Tuesday"));
        assert!(
            contains(&out, b"tiff:Make"),
            "the uncompressed XMP is blanked, not dropped"
        );
        // The png decoder checks CRCs; it reads the same picture.
        assert_eq!(
            codec::decode(&src).unwrap().raster,
            codec::decode(&out).unwrap().raster
        );
    }

    #[test]
    fn tiff_files_lose_their_gps_directory() {
        // A TIFF file is the EXIF structure itself: give the TIFF encoder's
        // output a GPS directory by pointing a tag at one appended to it.
        use std::io::Cursor;
        use tiff::encoder::{TiffEncoder, colortype};
        use tiff::tags::Tag;
        let r = sample();
        let mut buf = Cursor::new(Vec::new());
        {
            let mut enc = TiffEncoder::new(&mut buf).unwrap();
            let mut img = enc.new_image::<colortype::RGB8>(16, 12).unwrap();
            // Placeholder, patched below once the directory's place is known.
            img.encoder()
                .write_tag(Tag::Unknown(GPS_IFD), 0u32)
                .unwrap();
            img.write_data(&r.bytes).unwrap();
        }
        let mut src = buf.into_inner();
        while src.len() % 2 == 1 {
            src.push(0);
        }
        // Append a little-endian GPS directory and its data (the encoder
        // writes little-endian), then point the tag at it.
        let gps = src.len();
        let block = exif(true);
        let gps_in_block = 8 + 2 + 2 * 12 + 4;
        let data_in_block = gps_in_block + 2 + 2 * 12 + 4;
        let mut dir = block[gps_in_block..].to_vec();
        // The latitude's offset, rebased into this file.
        let lat_off = 2 + 12 + 8;
        let rebased = (gps + (data_in_block - gps_in_block)) as u32;
        dir[lat_off..lat_off + 4].copy_from_slice(&rebased.to_le_bytes());
        src.extend_from_slice(&dir);
        let t = Tiff::of(&src).unwrap();
        let (ifd, n) = t.chain(&src).unwrap()[0];
        let e = (0..n)
            .map(|k| ifd + 2 + k * 12)
            .find(|&e| t.u16(&src, e) == Some(GPS_IFD))
            .unwrap();
        src[e + 8..e + 12].copy_from_slice(&(gps as u32).to_le_bytes());

        assert!(contains(&src, &[0xA5; 24]));
        let out = remove_location(&src).unwrap();
        assert_no_location(&out);
        assert_eq!(t.u16(&out, gps), Some(0), "an empty GPS directory");
        assert_eq!(
            codec::decode(&src).unwrap().raster,
            codec::decode(&out).unwrap().raster
        );
    }

    #[test]
    fn photos_without_metadata_come_back_unchanged() {
        for f in [Format::Jpeg, Format::Png, Format::Tiff] {
            let b = codec::encode(&sample(), f, None).unwrap();
            assert_eq!(remove_location(&b).unwrap(), b, "{f}");
        }
    }

    #[test]
    fn unreadable_exif_is_refused_not_passed_on() {
        let mut ex = b"Exif\0\0".to_vec();
        let mut t = exif(true);
        t[4] = 0xF0; // IFD0 far past the end
        ex.extend_from_slice(&t);
        let plain = codec::encode(&sample(), Format::Jpeg, None).unwrap();
        let mut src = plain[..2].to_vec();
        src.extend(segment(0xE1, &ex));
        src.extend_from_slice(&plain[2..]);
        assert!(remove_location(&src).is_err());
    }

    #[test]
    fn xmp_forms() {
        let mut x = br#"<a exif:GPSLatitude="1" b:Other="keep"/> <exif:GPSTimeStamp>12:00</exif:GPSTimeStamp>"#.to_vec();
        blank_xmp(&mut x);
        assert_eq!(
            String::from_utf8(x).unwrap(),
            r#"<a exif:GPSLatitude=" " b:Other="keep"/> <exif:GPSTimeStamp>     </exif:GPSTimeStamp>"#
        );
    }
}
