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
//! Exports run the recipe on the full raster, exactly as the CLI does, and
//! so do exact previews and zoomed crops. A second cache keeps two
//! full-resolution rasters ([`FullCache`]): the picture just before the
//! step being edited, and the last finished render. Editing one step over
//! and over re-runs only that step and the ones after it; panning a zoomed
//! view, or exporting what the preview just showed, re-runs nothing.

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
    /// The same for zoomed crops, which run beside the whole-picture
    /// preview: neither may cancel the other.
    zoom_generation: AtomicU64,
    cache: Mutex<RenderCache>,
    full: Mutex<FullCache>,
    /// The work size last asked for, and the image shrunk to it (none at
    /// full size: the canvas is then the image itself, not a copy). Both
    /// caches above hold renders of this canvas, so they are cleared when
    /// it changes.
    canvas: Mutex<(Option<u32>, Option<Arc<Raster>>)>,
}

/// The raster a recipe's steps run on: the image itself, or a shrunk copy.
pub enum Canvas<'a> {
    Full(&'a Raster),
    Work(Arc<Raster>),
}

impl std::ops::Deref for Canvas<'_> {
    type Target = Raster;
    fn deref(&self) -> &Raster {
        match self {
            Canvas::Full(r) => r,
            Canvas::Work(r) => r,
        }
    }
}

#[derive(Default)]
struct RenderCache {
    base: Option<Base>,
}

/// A step as it ran: the step and the sample format it ran with.
type StepKey = (Step, Encoding, usize);

/// Full-resolution renders kept between requests. A 26 MP raster is about
/// 78 MB, so only two are kept, and opening another photo drops them.
#[derive(Default)]
struct FullCache {
    /// The steps of the last recipe asked for. The first step that differs
    /// in the next one is the step being edited.
    last: Vec<StepKey>,
    /// The picture after the first `keys.len()` steps of `keys`: just
    /// before the step being edited.
    before: Option<(Vec<StepKey>, Arc<Vec<u8>>)>,
    /// The last finished render, with all its steps.
    done: Option<(Vec<StepKey>, Arc<Raster>)>,
}

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
            generation: AtomicU64::new(0),
            zoom_generation: AtomicU64::new(0),
            cache: Mutex::new(RenderCache::default()),
            full: Mutex::new(FullCache::default()),
            canvas: Mutex::new((None, None)),
            image: Arc::new(image),
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

    /// The same pair for zoomed crops.
    pub fn begin_zoom(&self) -> u64 {
        self.zoom_generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn is_current_zoom(&self, generation: u64) -> bool {
        self.zoom_generation.load(Ordering::SeqCst) == generation
    }

    fn lock(&self) -> MutexGuard<'_, RenderCache> {
        // A panic while rendering leaves at worst a stale cache; recover it.
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn lock_full(&self) -> MutexGuard<'_, FullCache> {
        self.full.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Free the full-resolution rasters (another photo is being worked on).
    fn drop_full(&self) {
        *self.lock_full() = FullCache::default();
    }

    /// The raster `recipe`'s steps run on: the image, shrunk to its work
    /// size. A new work size drops every cached render.
    pub fn canvas(&self, recipe: &Recipe) -> Canvas<'_> {
        let mut c = self.canvas.lock().unwrap_or_else(|p| p.into_inner());
        if c.0 != recipe.work_size {
            let size = recipe.canvas_size(self.image.raster.width, self.image.raster.height);
            let shrunk = (size != (self.image.raster.width, self.image.raster.height))
                .then(|| Arc::new(recipe.canvas(&self.image.raster)));
            *c = (recipe.work_size, shrunk);
            *self.lock() = RenderCache::default();
            self.drop_full();
        }
        match &c.1 {
            Some(r) => Canvas::Work(r.clone()),
            None => Canvas::Full(&self.image.raster),
        }
    }

    /// Units for a raster of `w` x `h` standing in for `canvas`.
    fn units(canvas: &Raster, w: usize, h: usize) -> Units {
        Units {
            full_width: canvas.width,
            full_height: canvas.height,
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
        let canvas = self.canvas(recipe);
        let size = canvas.fit(max_w, max_h);

        let mut cache = self.lock();
        if !matches!(&cache.base, Some(b) if (b.raster.width, b.raster.height) == size) {
            cache.base = Some(Base {
                raster: canvas.resize(size.0, size.1),
                steps: Vec::new(),
            });
        }
        let base = cache.base.as_mut().expect("base was just set");
        let keys = step_keys(recipe, &prepared);

        // Longest prefix of steps that is already cached.
        let keep = base
            .steps
            .iter()
            .zip(&keys)
            .take_while(|((cached, _), key)| cached == *key)
            .count();
        // Drop the cache from the first difference on. A recipe that is a
        // prefix of the cached chain (the mask view asks for the picture as
        // it arrives at a step) differs nowhere, so the tail is kept.
        if keep < keys.len() {
            base.steps.truncate(keep);
        }

        let mut bytes = match keep {
            0 => base.raster.bytes.clone(),
            n => base.steps[n - 1].1.clone(),
        };
        let units = Self::units(&canvas, size.0, size.1);
        let max_cached = (STEP_CACHE_BYTES / bytes.len().max(1)).max(1);
        for (key, p) in keys.into_iter().zip(&prepared).skip(keep) {
            p.apply(&mut bytes, &units, cancel)?;
            if base.steps.len() < max_cached {
                base.steps.push((key, bytes.clone()));
            }
        }
        Ok(Raster::new(size.0, size.1, bytes))
    }

    /// Step `index`'s mask at preview size, as coverage 0..1 per pixel, with
    /// the preview's width and height. Brightness operations read the
    /// picture as it arrives at the step, which comes from the cache. A step
    /// without a mask shows everywhere.
    pub fn mask(
        &self,
        recipe: &Recipe,
        index: usize,
        max_w: usize,
        max_h: usize,
        cancel: Cancel<'_>,
    ) -> Result<(usize, usize, Vec<f32>)> {
        let step = recipe.steps.get(index).ok_or_else(|| {
            glitchrx_core::Error::Recipe(format!("there is no step {}", index + 1))
        })?;
        recipe.validate()?;
        let before = Recipe {
            steps: recipe.steps[..index].to_vec(),
            ..recipe.clone()
        };
        let input = self.preview(&before, max_w, max_h, cancel)?;
        let (w, h) = (input.width, input.height);
        let coverage = match &step.mask {
            Some(m) => m.render(w, h, &input.bytes, cancel)?,
            None => vec![1.0; w * h],
        };
        Ok((w, h, coverage))
    }

    /// Bend the full-resolution image, exactly as the CLI would, starting
    /// from the cache where it can ([`FullCache`]). Running the steps after
    /// a cached intermediate gives the same bytes as running them all,
    /// since each step only sees the bytes the one before it left.
    pub fn render_full(&self, recipe: &Recipe, cancel: Cancel<'_>) -> Result<Arc<Raster>> {
        let prepared = recipe.prepare_all()?;
        let canvas = self.canvas(recipe);
        let keys = step_keys(recipe, &prepared);
        let (start, mut bytes, edit) = {
            let mut full = self.lock_full();
            if let Some((done, raster)) = &full.done
                && *done == keys
            {
                return Ok(raster.clone());
            }
            // With nothing to compare with yet, guess the last step: it is
            // the one usually being tuned.
            let edit = if full.last.is_empty() {
                keys.len().saturating_sub(1)
            } else {
                full.last
                    .iter()
                    .zip(&keys)
                    .take_while(|(a, b)| a == b)
                    .count()
            };
            full.last = keys.clone();
            match &full.before {
                Some((k, b)) if keys.starts_with(k) => (k.len(), b.as_ref().clone(), edit),
                _ => (0, canvas.bytes.clone(), edit),
            }
        };
        let (w, h) = (canvas.width, canvas.height);
        let units = Self::units(&canvas, w, h);
        for (i, p) in prepared.iter().enumerate().skip(start) {
            if i == edit && i > start {
                // Keep the picture as it arrives at the step being edited,
                // so the next change to it starts here. Stored at once, so
                // it survives a cancel.
                self.lock_full().before = Some((keys[..i].to_vec(), Arc::new(bytes.clone())));
            }
            p.apply(&mut bytes, &units, cancel)?;
        }
        let raster = Arc::new(Raster::new(w, h, bytes));
        self.lock_full().done = Some((keys, raster.clone()));
        Ok(raster)
    }
}

/// Each step of `recipe` with the sample format it runs with: what decides
/// the bytes it gives, and so what the caches compare.
fn step_keys(recipe: &Recipe, prepared: &[glitchrx_core::recipe::Prepared]) -> Vec<StepKey> {
    recipe
        .steps
        .iter()
        .zip(prepared)
        .map(|(s, p)| (s.clone(), p.encoding, p.channels))
        .collect()
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

    /// Add a document, closing the least recently used one if full. The
    /// others keep their preview caches but drop their full-resolution
    /// rasters: only the photo being worked on needs them.
    pub fn insert(&self, doc: Doc) -> Arc<Doc> {
        let doc = Arc::new(doc);
        let mut docs = self.lock();
        for other in docs.iter() {
            other.drop_full();
        }
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
                let units = Doc::units(&d.image.raster, want.width, want.height);
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
        assert_eq!(*d.render_full(&r, NEVER).unwrap(), want);
    }

    /// Whatever order edits arrive in, and even after a cancelled render,
    /// the cached full render equals a fresh run of the recipe.
    #[test]
    fn cached_full_render_matches_a_fresh_run() {
        let d = doc("baseline-420.jpg");
        let sequence = [
            recipe(
                &[("echo", "", 1), ("shift", "", 2), ("stutter", "", 3)],
                Encoding::U8,
            ),
            // Edit the last step twice: the second starts from the cache.
            recipe(
                &[("echo", "", 1), ("shift", "", 2), ("stutter", "", 4)],
                Encoding::U8,
            ),
            recipe(
                &[("echo", "", 1), ("shift", "", 2), ("stutter", "", 5)],
                Encoding::U8,
            ),
            // Then an earlier one, then one past the cached picture.
            recipe(
                &[("echo", "", 1), ("shift", "by=9", 2), ("stutter", "", 5)],
                Encoding::U8,
            ),
            recipe(
                &[("echo", "", 1), ("shift", "by=9", 2), ("stutter", "", 6)],
                Encoding::U8,
            ),
            // A step added, one removed, the encoding changed, none left.
            recipe(
                &[
                    ("echo", "", 1),
                    ("shift", "by=9", 2),
                    ("stutter", "", 6),
                    ("invert", "", 7),
                ],
                Encoding::U8,
            ),
            recipe(&[("echo", "", 1), ("shift", "by=9", 2)], Encoding::U8),
            recipe(&[("echo", "", 1), ("shift", "by=9", 2)], Encoding::ULaw),
            recipe(&[], Encoding::U8),
            recipe(&[("echo", "", 1), ("shift", "by=9", 2)], Encoding::U8),
        ];
        for (i, r) in sequence.iter().enumerate() {
            if i == 4 {
                // Cancel part of the way through first.
                let calls = AtomicU64::new(0);
                let stop = || calls.fetch_add(1, Ordering::SeqCst) >= 3;
                assert!(d.render_full(r, &stop).is_err());
            }
            let got = d.render_full(r, NEVER).unwrap();
            let mut want = d.image.raster.clone();
            r.apply(&mut want).unwrap();
            assert_eq!(*got, want, "edit {i}: {r:?}");
        }
    }

    /// Editing one step keeps the picture before it; asking again for the
    /// same recipe (a pan, an export) gives back the finished render.
    #[test]
    fn full_cache_keeps_the_edit_point_and_the_result() {
        let d = doc("baseline-420.jpg");
        let at = |s: u64| {
            recipe(
                &[("echo", "", 1), ("shift", "", 2), ("stutter", "", s)],
                Encoding::U8,
            )
        };
        let before = || d.lock_full().before.as_ref().map(|b| b.0.len());
        d.render_full(&at(3), NEVER).unwrap();
        assert_eq!(before(), Some(2), "a first render guesses the last step");
        d.render_full(&at(4), NEVER).unwrap();
        assert_eq!(before(), Some(2));
        // Editing the middle step moves the kept picture back to it.
        let mut middle = at(4);
        middle.steps[1] = Step::new("shift", "by=9", 2).unwrap();
        d.render_full(&middle, NEVER).unwrap();
        assert_eq!(before(), Some(1));
        let a = d.render_full(&at(5), NEVER).unwrap();
        let b = d.render_full(&at(5), NEVER).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "the same recipe is not rendered twice");
    }

    #[test]
    fn opening_a_photo_drops_the_others_full_renders() {
        let store = Store::new(3);
        let a = store.insert(doc("baseline-420.jpg"));
        a.render_full(&recipe(&[("echo", "", 1)], Encoding::U8), NEVER)
            .unwrap();
        assert!(a.lock_full().done.is_some());
        store.insert(doc("grayscale.jpg"));
        assert!(a.lock_full().done.is_none());
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

    /// Asking for the picture before a step (the mask view does) keeps the
    /// steps after it cached.
    #[test]
    fn a_prefix_keeps_the_cached_tail() {
        let d = doc("baseline-420.jpg");
        let full = recipe(
            &[("echo", "", 1), ("shift", "", 2), ("stutter", "", 3)],
            Encoding::U8,
        );
        d.preview(&full, 120, 120, NEVER).unwrap();
        d.mask(&full, 1, 120, 120, NEVER).unwrap();
        let cached = d.lock().base.as_ref().unwrap().steps.len();
        assert_eq!(cached, 3);
    }
}
