//! Project files (`.glrx`, SPEC 8.1, D-63): an edit that opens again
//! anywhere, as a `.psd` does.
//!
//! A recipe replays on the original photo, so a project carries both, and a
//! preview to show before anything is rendered. The container is a zip, as
//! OpenDocument, EPUB, Krita and OpenRaster files are:
//!
//! ```text
//! mimetype        first and uncompressed: what the file is, readable at a
//!                 fixed place in its first bytes (the OpenDocument trick)
//! project.toml    the manifest: format, version, where the original is
//! recipe.toml     the recipe (SPEC 8)
//! original.jpg    the original photo (or .png, .tif), its location removed
//! preview.jpg     the finished picture, 1024 px on the longer side
//! ```
//!
//! Reading takes only these entries, by exact name, into memory: nothing is
//! ever unpacked onto a disk, so names like `../x` cannot do any harm. Every
//! entry has a size cap, checked against what is actually read rather than
//! what the zip claims, so a small file cannot unpack into gigabytes.
//!
//! Writing is deterministic: the same project gives the same bytes (fixed
//! timestamps, entries in a fixed order).

use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

use crate::codec::{self, EncodeSettings, Format};
use crate::error::{Error, Result};
use crate::meta;
use crate::raster::Raster;
use crate::recipe::Recipe;

/// The file extension, without the dot.
pub const EXTENSION: &str = "glrx";
/// The `mimetype` entry's content. Not registered with IANA; the `vnd.`
/// tree is where a registration would go.
pub const MIMETYPE: &str = "application/vnd.glitcharaxia.project";
/// The manifest's `format`.
const FORMAT: &str = "glitcharaxia-project";
/// The format version this build writes, and the newest it reads.
pub const VERSION: u32 = 1;

/// Size caps for reading.
const MAX_ORIGINAL: u64 = 256 << 20; // the server's upload limit
const MAX_TEXT: u64 = 1 << 20;
const MAX_PREVIEW: u64 = 8 << 20;
/// A project has five entries; a zip with far more is not one of ours.
const MAX_ENTRIES: usize = 64;

/// The longer side of the preview inside a project.
pub const PREVIEW_SIDE: usize = 1024;

/// The preview a project carries: `picture` (the finished render, at any
/// size) shrunk to [`PREVIEW_SIDE`] and encoded as a JPEG, with the photo's
/// color profile so it looks as the export does.
pub fn preview(picture: &Raster, icc: Option<&[u8]>) -> Result<Vec<u8>> {
    let small = picture.downscale(PREVIEW_SIDE, PREVIEW_SIDE);
    let settings = EncodeSettings {
        quality: Some(85),
        ..EncodeSettings::default()
    };
    codec::encode_with(&small, Format::Jpeg, icc, &settings)
}

/// An edit: the original photo, the recipe and a preview.
#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub recipe: Recipe,
    /// The original's file name, as it was opened (`IMG_1234.jpg`).
    pub name: String,
    /// The original file: its image data exactly as it was, its location
    /// removed (see [`meta`]).
    pub original: Vec<u8>,
    /// The finished picture as a JPEG, if the project has one. Not needed
    /// to replay.
    pub preview: Option<Vec<u8>>,
    /// What wrote it (`glitchrx 0.1.0`), when read from a file.
    pub writer: Option<String>,
}

/// `project.toml`.
#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    format: String,
    version: u32,
    /// The entry holding the original (`original.jpg`).
    original: String,
    /// The original's own file name.
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    writer: Option<String>,
}

fn bad(msg: impl Into<String>) -> Error {
    Error::Project(msg.into())
}

/// The original's entry name for a format.
fn original_entry(f: Format) -> &'static str {
    match f {
        Format::Png => "original.png",
        Format::Tiff => "original.tif",
        _ => "original.jpg",
    }
}

impl Project {
    /// A new project from the original file's bytes. Its location is
    /// removed here, so every way of making a project removes it. The
    /// recipe's `source` becomes the original's name.
    pub fn new(
        mut recipe: Recipe,
        name: &str,
        original: &[u8],
        preview: Option<Vec<u8>>,
    ) -> Result<Project> {
        match Format::sniff(original) {
            Some(f) if f.readable() => {}
            _ => return Err(Error::UnknownFormat),
        }
        recipe.source = Some(name.to_string());
        Ok(Project {
            recipe,
            name: name.to_string(),
            original: meta::remove_location(original)?,
            preview,
            writer: None,
        })
    }

    /// The `.glrx` file's bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let format = Format::sniff(&self.original).ok_or(Error::UnknownFormat)?;
        let entry = original_entry(format);
        let manifest = Manifest {
            format: FORMAT.into(),
            version: VERSION,
            original: entry.into(),
            name: self.name.clone(),
            writer: Some(format!("glitchrx {}", env!("CARGO_PKG_VERSION"))),
        };
        let manifest = toml::to_string(&manifest).expect("manifest serialization cannot fail");

        // A fixed time keeps the bytes the same from one save to the next.
        let base = SimpleFileOptions::default().last_modified_time(DateTime::default());
        let stored = base.compression_method(CompressionMethod::Stored);
        let deflated = base.compression_method(CompressionMethod::Deflated);

        let zip_err = |e: zip::result::ZipError| bad(format!("cannot write: {e}"));
        let io_err = |e: std::io::Error| bad(format!("cannot write: {e}"));
        let mut z = ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("mimetype", stored).map_err(zip_err)?;
        z.write_all(MIMETYPE.as_bytes()).map_err(io_err)?;
        z.start_file("project.toml", deflated).map_err(zip_err)?;
        z.write_all(manifest.as_bytes()).map_err(io_err)?;
        z.start_file("recipe.toml", deflated).map_err(zip_err)?;
        z.write_all(self.recipe.to_toml().as_bytes())
            .map_err(io_err)?;
        // Photos are compressed already: stored, they read back faster.
        z.start_file(entry, stored).map_err(zip_err)?;
        z.write_all(&self.original).map_err(io_err)?;
        if let Some(p) = &self.preview {
            z.start_file("preview.jpg", stored).map_err(zip_err)?;
            z.write_all(p).map_err(io_err)?;
        }
        Ok(z.finish().map_err(zip_err)?.into_inner())
    }

    /// Read a `.glrx` file.
    pub fn from_bytes(bytes: &[u8]) -> Result<Project> {
        if !is_project(bytes) {
            return Err(bad("not a glitcharaxia project"));
        }
        let mut z =
            ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(format!("damaged: {e}")))?;
        if z.len() > MAX_ENTRIES {
            return Err(bad("damaged: too many entries"));
        }

        let text = |z: &mut ZipArchive<Cursor<&[u8]>>, name: &str| -> Result<String> {
            String::from_utf8(entry(z, name, MAX_TEXT)?)
                .map_err(|_| bad(format!("{name} is not text")))
        };
        let m: Manifest = toml::from_str(&text(&mut z, "project.toml")?)
            .map_err(|e| bad(format!("project.toml: {e}")))?;
        if m.format != FORMAT {
            return Err(bad("not a glitcharaxia project"));
        }
        if m.version > VERSION {
            return Err(bad(format!(
                "made by a newer glitchrx (project version {}; this one reads up to {VERSION})",
                m.version
            )));
        }
        if !["original.jpg", "original.png", "original.tif"].contains(&m.original.as_str()) {
            return Err(bad(format!("unexpected original entry `{}`", m.original)));
        }
        let recipe = Recipe::from_toml(&text(&mut z, "recipe.toml")?)?;
        let original = entry(&mut z, &m.original, MAX_ORIGINAL)?;
        if Format::sniff(&original).is_none_or(|f| !f.readable()) {
            return Err(Error::UnknownFormat);
        }
        let preview = if z.index_for_name("preview.jpg").is_some() {
            Some(entry(&mut z, "preview.jpg", MAX_PREVIEW)?)
        } else {
            None
        };
        Ok(Project {
            recipe,
            name: m.name,
            original,
            preview,
            writer: m.writer,
        })
    }
}

/// One entry, at most `cap` bytes, however large the zip says it is.
fn entry(z: &mut ZipArchive<Cursor<&[u8]>>, name: &str, cap: u64) -> Result<Vec<u8>> {
    let f = z
        .by_name(name)
        .map_err(|_| bad(format!("{name} is missing")))?;
    let too_big = || bad(format!("{name} is too large"));
    if f.size() > cap {
        return Err(too_big());
    }
    let mut out = Vec::with_capacity(f.size() as usize);
    f.take(cap + 1)
        .read_to_end(&mut out)
        .map_err(|e| bad(format!("{name}: {e}")))?;
    if out.len() as u64 > cap {
        return Err(too_big());
    }
    Ok(out)
}

/// Whether `bytes` start like a project: a zip whose first entry is the
/// uncompressed `mimetype`. Ours has it at a fixed place (after the 30-byte
/// local header and the 8-byte name), which is what file managers look
/// for; the lengths are read anyway, in case another zip tool rewrote it.
pub fn is_project(bytes: &[u8]) -> bool {
    let u16_at = |i: usize| {
        bytes
            .get(i..i + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
    };
    let (Some(method), Some(name), Some(extra)) = (u16_at(8), u16_at(26), u16_at(28)) else {
        return false;
    };
    let data = 30 + name + extra;
    bytes.starts_with(b"PK\x03\x04")
        && method == 0
        && bytes.get(30..30 + name) == Some(b"mimetype")
        && bytes.get(data..data + MIMETYPE.len()) == Some(MIMETYPE.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photo(f: Format) -> Vec<u8> {
        let bytes = (0..20 * 10)
            .flat_map(|i| [(i * 3) as u8, (i * 5) as u8, (i * 7) as u8])
            .collect();
        codec::encode(&Raster::new(20, 10, bytes), f, None).unwrap()
    }

    fn recipe() -> Recipe {
        Recipe::from_toml(
            "format = \"glitcharaxia-recipe\"\nversion = 4\n[[step]]\neffect = \"echo\"\neffect_version = 1\nseed = 7\n",
        )
        .unwrap()
    }

    #[test]
    fn round_trip_for_each_format() {
        for f in [Format::Jpeg, Format::Png, Format::Tiff] {
            let original = photo(f);
            let p =
                Project::new(recipe(), "IMG_1.x", &original, Some(photo(Format::Jpeg))).unwrap();
            let bytes = p.to_bytes().unwrap();
            assert!(is_project(&bytes), "{f}");
            let back = Project::from_bytes(&bytes).unwrap();
            assert_eq!(
                back.original, original,
                "{f}: no metadata here, so byte for byte"
            );
            assert_eq!(back.recipe, p.recipe);
            assert_eq!(back.recipe.source.as_deref(), Some("IMG_1.x"));
            assert_eq!(back.name, "IMG_1.x");
            assert_eq!(back.preview, p.preview);
            assert_eq!(
                back.writer.as_deref(),
                Some(concat!("glitchrx ", env!("CARGO_PKG_VERSION")))
            );
            // The same project writes the same bytes.
            assert_eq!(p.to_bytes().unwrap(), bytes);
        }
    }

    #[test]
    fn the_original_loses_its_location_on_the_way_in() {
        let src = crate::meta::tests::located_jpeg(true);
        let p = Project::new(recipe(), "a.jpg", &src, None).unwrap();
        let back = Project::from_bytes(&p.to_bytes().unwrap()).unwrap();
        assert_ne!(back.original, src);
        assert_eq!(back.original.len(), src.len());
        assert_eq!(
            codec::decode(&back.original).unwrap().raster,
            codec::decode(&src).unwrap().raster
        );
        assert!(back.preview.is_none());
    }

    /// A zip in our layout with any entries, for the refusals below.
    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let o = SimpleFileOptions::default()
            .last_modified_time(DateTime::default())
            .compression_method(CompressionMethod::Stored);
        let mut z = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            z.start_file(*name, o).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap().into_inner()
    }

    fn manifest(version: u32, original: &str) -> String {
        format!(
            "format = \"{FORMAT}\"\nversion = {version}\noriginal = \"{original}\"\nname = \"a.jpg\"\n"
        )
    }

    #[test]
    fn refuses_what_is_not_a_project() {
        assert!(Project::from_bytes(b"not a zip").is_err());
        assert!(Project::from_bytes(&photo(Format::Jpeg)).is_err());
        // A zip, but no mimetype first.
        let r = recipe().to_toml();
        let z = zip_of(&[
            ("project.toml", manifest(1, "original.jpg").as_bytes()),
            ("recipe.toml", r.as_bytes()),
        ]);
        assert!(!is_project(&z));
        assert!(Project::from_bytes(&z).is_err());
    }

    #[test]
    fn refuses_a_newer_version_with_a_message() {
        let r = recipe().to_toml();
        let jpg = photo(Format::Jpeg);
        let z = zip_of(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("project.toml", manifest(2, "original.jpg").as_bytes()),
            ("recipe.toml", r.as_bytes()),
            ("original.jpg", &jpg),
        ]);
        let e = Project::from_bytes(&z).unwrap_err().to_string();
        assert!(e.contains("newer glitchrx"), "{e}");
    }

    #[test]
    fn refuses_odd_entry_names_and_missing_entries() {
        let r = recipe().to_toml();
        let jpg = photo(Format::Jpeg);
        let z = zip_of(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("project.toml", manifest(1, "../../etc/passwd").as_bytes()),
            ("recipe.toml", r.as_bytes()),
            ("../../etc/passwd", &jpg),
        ]);
        assert!(Project::from_bytes(&z).is_err());
        let z = zip_of(&[
            ("mimetype", MIMETYPE.as_bytes()),
            ("project.toml", manifest(1, "original.jpg").as_bytes()),
            ("recipe.toml", r.as_bytes()),
        ]);
        let e = Project::from_bytes(&z).unwrap_err().to_string();
        assert!(e.contains("original.jpg is missing"), "{e}");
    }

    #[test]
    fn caps_what_an_entry_may_unpack_to() {
        // 2 MB of one byte deflates to a few kilobytes: a small zip bomb
        // for the 1 MB text cap.
        let big = vec![b'#'; 2 << 20];
        let jpg = photo(Format::Jpeg);
        let o = SimpleFileOptions::default().last_modified_time(DateTime::default());
        let mut z = ZipWriter::new(Cursor::new(Vec::new()));
        z.start_file("mimetype", o.compression_method(CompressionMethod::Stored))
            .unwrap();
        z.write_all(MIMETYPE.as_bytes()).unwrap();
        z.start_file(
            "project.toml",
            o.compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        z.write_all(manifest(1, "original.jpg").as_bytes()).unwrap();
        z.start_file(
            "recipe.toml",
            o.compression_method(CompressionMethod::Deflated),
        )
        .unwrap();
        z.write_all(&big).unwrap();
        z.start_file("original.jpg", o).unwrap();
        z.write_all(&jpg).unwrap();
        let bytes = z.finish().unwrap().into_inner();
        assert!(bytes.len() < 64 << 10);
        let e = Project::from_bytes(&bytes).unwrap_err().to_string();
        assert!(e.contains("too large"), "{e}");
    }

    #[test]
    fn the_mimetype_sits_where_file_managers_look() {
        let p = Project::new(recipe(), "a.jpg", &photo(Format::Jpeg), None).unwrap();
        let b = p.to_bytes().unwrap();
        assert_eq!(&b[30..38], b"mimetype");
        assert_eq!(&b[38..38 + MIMETYPE.len()], MIMETYPE.as_bytes());
    }
}
