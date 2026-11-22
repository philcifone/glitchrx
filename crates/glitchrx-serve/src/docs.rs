//! Open documents and their render cache.
//!
//! A *document* is one photo opened in the GUI: the decoded image, where it
//! came from, and a cache that makes re-rendering cheap while a slider moves.
//!
//! Previews run the recipe on a screen-sized copy of the image (SPEC D-26):
//! a 26 MP photo shrunk to a phone screen is about 1/20 of the samples, so
//! even heavy steps answer quickly. Sizes in the recipe are mapped to the
//! smaller raster ([`Units`]), so the preview looks like the export without
//! being byte-identical to it.
//!
//! The cache keeps the preview-sized base and the raster after each step,
//! keyed by the step itself (with its encoding), so when only step 3 of 4
//! changes, steps 1 and 2 are not re-run. A step carries its own seed, so
//! "the same step" really does mean "the same bytes out".
//!
//! Exports run the recipe on the full raster, exactly as the CLI does.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use glitchrx_core::codec::{self, Image};
use glitchrx_core::recipe::Cancel;
use glitchrx_core::{Encoding, Raster, Recipe, Result, Step, Units};

/// Upper bound on cached per-step preview rasters, in bytes, per document.
/// A 4096-pixel preview is about 50 MB, a phone-sized one about 5 MB.
const STEP_CACHE_BYTES: usize = 96 << 20;

/// One open photo.
pub struct Doc {
    pub id: String,
    /// File name shown in the UI and used to name downloads.
    pub name: String,
    /// Absolute path when the photo was opened from the server's `--root`
    /// (so results can be saved next to it); `None` for uploads.
    pub source: Option<PathBuf>,
    /// Size of the original file.
    pub file_len: usize,
    /// The decoded, upright image.
    pub image: Arc<Image>,
    /// Bumped by every preview request; a request that finds it changed by
    /// the time it gets to run has been superseded and is dropped.
    generation: AtomicU64,
    cache: Mutex<RenderCache>,
}

#[derive(Default)]
struct RenderCache {
    base: Option<Base>,
}

/// A step as it ran: the step and the sample format it ran with.
type StepKey = (Step, Encoding, usize);

/// The preview-sized original plus per-step snapshots.
struct Base {
    raster: Raster,
    /// `steps[i]` holds step `i` and the raster after steps `0..=i`.
    steps: Vec<(StepKey, Vec<u8>)>,
}

impl Doc {
    /// Decode `bytes` (JPEG, PNG or TIFF) and wrap it as a document.
    pub fn new(id: String, name: String, source: Option<PathBuf>, bytes: &[u8]) -> Result<Doc> {
        let image = codec::decode(bytes)?;
        Ok(Doc {
            id,
            name,
            source,
            file_len: bytes.len(),
            image: Arc::new(image),
            generation: AtomicU64::new(0),
            cache: Mutex::new(RenderCache::default()),
        })
    }

    /// Register a new render request and return its generation number.
    pub fn begin(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// True if no newer request arrived after `generation`.
    pub fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::SeqCst) == generation
    }

    fn lock(&self) -> MutexGuard<'_, RenderCache> {
        // A panic while rendering leaves at worst a stale cache; recover it.
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Units for a raster of `w` x `h` standing in for this image.
    fn units(&self, w: usize, h: usize) -> Units {
        Units {
            full_width: self.image.raster.width,
            full_height: self.image.raster.height,
            width: w,
            height: h,
        }
    }

    /// The untouched image, fitted to `max_w` x `max_h`.
    pub fn original(&self, max_w: usize, max_h: usize) -> Raster {
        self.image.raster.downscale(max_w, max_h)
    }

    /// Bend a preview-sized copy with `recipe`, reusing cached work. Stops
    /// with [`glitchrx_core::Error::Cancelled`] when `cancel` says so; the
    /// steps finished so far stay cached.
    pub fn preview(
        &self,
        recipe: &Recipe,
        max_w: usize,
        max_h: usize,
        cancel: Cancel<'_>,
    ) -> Result<Raster> {
        // Validate every step before touching the cache.
        let prepared = recipe.prepare_all()?;
        let size = self.image.raster.fit(max_w, max_h);

        let mut cache = self.lock();
        if !matches!(&cache.base, Some(b) if (b.raster.width, b.raster.height) == size) {
            cache.base = Some(Base {
                raster: self.image.raster.resize(size.0, size.1),
                steps: Vec::new(),
            });
        }
        let base = cache.base.as_mut().expect("base was just set");
        let keys: Vec<StepKey> = recipe
            .steps
            .iter()
            .zip(&prepared)
            .map(|(s, p)| (s.clone(), p.encoding, p.channels))
            .collect();

        // Longest prefix of steps that is already cached.
        let keep = base
            .steps
            .iter()
            .zip(&keys)
            .take_while(|((cached, _), key)| cached == *key)
            .count();
        base.steps.truncate(keep);

        let mut bytes = match keep {
            0 => base.raster.bytes.clone(),
            n => base.steps[n - 1].1.clone(),
        };
        let units = self.units(size.0, size.1);
        let max_cached = (STEP_CACHE_BYTES / bytes.len().max(1)).max(1);
        for (key, p) in keys.into_iter().zip(&prepared).skip(keep) {
            p.apply(&mut bytes, &units, cancel)?;
            if base.steps.len() < max_cached {
                base.steps.push((key, bytes.clone()));
            }
        }
        Ok(Raster::new(size.0, size.1, bytes))
    }

    /// Bend the full-resolution image, exactly as the CLI would.
    pub fn render_full(&self, recipe: &Recipe, cancel: Cancel<'_>) -> Result<Raster> {
        let mut raster = self.image.raster.clone();
        let units = self.units(raster.width, raster.height);
        recipe.apply_with(&mut raster.bytes, &units, cancel)?;
        Ok(raster)
    }
}

/// The set of open documents, most recently used first.
pub struct Store {
    docs: Mutex<VecDeque<Arc<Doc>>>,
    max_docs: usize,
}

impl Store {
    pub fn new(max_docs: usize) -> Store {
        Store {
            docs: Mutex::new(VecDeque::new()),
            max_docs: max_docs.max(1),
        }
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<Arc<Doc>>> {
        self.docs.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Add a document, closing the least recently used one if full.
    pub fn insert(&self, doc: Doc) -> Arc<Doc> {
        let doc = Arc::new(doc);
        let mut docs = self.lock();
        docs.push_front(doc.clone());
        docs.truncate(self.max_docs);
        doc
    }

    /// Look up a document and mark it as recently used.
    pub fn get(&self, id: &str) -> Option<Arc<Doc>> {
        let mut docs = self.lock();
        let pos = docs.iter().position(|d| d.id == id)?;
        let doc = docs.remove(pos)?;
        docs.push_front(doc.clone());
        Some(doc)
    }
}

/// A random hex string with `bytes` bytes of entropy, for document ids and
/// auto-generated tokens. Reads the OS generator where there is one.
pub fn random_hex(bytes: usize) -> String {
    use std::hash::{BuildHasher, RandomState};
    use std::io::Read;

    let mut buf = vec![0u8; bytes];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .is_ok();
    if !ok {
        // RandomState is keyed from OS randomness once per process; hashing
        // a counter with fresh states gives unpredictable bytes everywhere.
        for (i, chunk) in buf.chunks_mut(8).enumerate() {
            let h = RandomState::new().hash_one(i).to_le_bytes();
            chunk.copy_from_slice(&h[..chunk.len()]);
        }
    }
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use glitchrx_core::recipe::NEVER;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).unwrap()
    }

    fn doc(name: &str) -> Doc {
        Doc::new("t".into(), name.into(), None, &fixture(name)).unwrap()
    }

    fn recipe(steps: &[(&str, &str, u64)], encoding: Encoding) -> Recipe {
        Recipe {
            encoding,
            steps: steps
                .iter()
                .map(|(e, p, s)| Step::new(e, p, *s).unwrap())
                .collect(),
            ..Recipe::default()
        }
    }

    /// Whatever order edits arrive in, the cached preview equals a fresh
    /// run of the recipe on the same preview-sized raster.
    #[test]
    fn cached_preview_matches_a_fresh_run() {
        let d = doc("baseline-420.jpg");
        let sequence = [
            recipe(&[("echo", "", 1)], Encoding::U8),
            recipe(&[("echo", "", 1), ("shift", "", 2)], Encoding::U8),
            recipe(&[("echo", "", 1), ("shift", "by=50", 2)], Encoding::U8),
            recipe(
                &[("echo", "", 1), ("shift", "by=50", 2), ("stutter", "", 3)],
                Encoding::U8,
            ),
            recipe(
                &[("echo", "decay=0.9", 1), ("shift", "by=50", 2)],
                Encoding::U8,
            ),
            // Same steps, different recipe-wide encoding: must not reuse.
            recipe(
                &[("echo", "decay=0.9", 1), ("shift", "by=50", 2)],
                Encoding::ULaw,
            ),
            recipe(&[], Encoding::U8),
            recipe(&[("backwards", "", 4)], Encoding::S16Le),
        ];
        for (w, h) in [(120, 120), (480, 480), (120, 120)] {
            for r in &sequence {
                let got = d.preview(r, w, h, NEVER).unwrap();
                let mut want = d.original(w, h);
                let units = d.units(want.width, want.height);
                r.apply_with(&mut want.bytes, &units, NEVER).unwrap();
                assert_eq!(got, want, "{r:?} at {w}x{h}");
            }
        }
    }

    #[test]
    fn full_render_matches_apply() {
        let d = doc("rgb8.png");
        let r = recipe(&[("reverb", "", 9)], Encoding::S8);
        let mut want = d.image.raster.clone();
        r.apply(&mut want).unwrap();
        assert_eq!(d.render_full(&r, NEVER).unwrap(), want);
    }

    #[test]
    fn cancelled_preview_is_an_error_and_cache_survives() {
        let d = doc("baseline-420.jpg");
        let r = recipe(&[("echo", "", 1)], Encoding::U8);
        let err = d.preview(&r, 200, 200, &|| true);
        assert!(matches!(err, Err(glitchrx_core::Error::Cancelled)));
        assert!(d.preview(&r, 200, 200, NEVER).is_ok());
    }

    #[test]
    fn bad_step_is_an_error_not_a_panic() {
        let d = doc("grayscale.jpg");
        let mut r = recipe(&[("echo", "", 1)], Encoding::U8);
        r.steps[0].params.insert("decay".into(), "7".into());
        assert!(d.preview(&r, 100, 100, NEVER).is_err());
    }

    #[test]
    fn store_evicts_least_recently_used() {
        let store = Store::new(2);
        let make = |id: &str| {
            Doc::new(id.into(), "x.jpg".into(), None, &fixture("grayscale.jpg")).unwrap()
        };
        store.insert(make("a"));
        store.insert(make("b"));
        assert!(store.get("a").is_some()); // a is now most recent
        store.insert(make("c")); // evicts b
        assert!(store.get("b").is_none());
        assert!(store.get("a").is_some());
        assert!(store.get("c").is_some());
    }

    #[test]
    fn generations_supersede() {
        let d = doc("grayscale.jpg");
        let first = d.begin();
        let second = d.begin();
        assert!(!d.is_current(first));
        assert!(d.is_current(second));
    }

    #[test]
    fn random_hex_has_requested_length() {
        let a = random_hex(16);
        assert_eq!(a.len(), 32);
        assert_ne!(a, random_hex(16));
    }
}
