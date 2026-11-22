//! Open documents and their render cache.
//!
//! A *document* is one photo opened in the GUI: its original bytes, where it
//! came from, and a cache that makes re-rendering cheap while a slider moves.
//!
//! The cache follows SPEC 10.2. Rendering a recipe means:
//!
//! 1. *Prepare* the input (add restart markers) and parse it. This is the
//!    slow part (a lossless transform of the whole file), so its result is
//!    kept until `restart` or `mode` changes.
//! 2. Run the steps over the payload. The payload after each step is cached,
//!    keyed by the step itself, so when only step 3 of 4 changes, steps 1 and
//!    2 are not re-run. Because a step carries its own seed, "the same step"
//!    really does mean "the same bytes out".
//! 3. Re-stuff and reassemble the file.
//!
//! The result is byte-identical to [`Recipe::apply`] on the original (a test
//! checks this), so what the GUI previews is exactly what the CLI would write.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use glitchrx_core::formats::jpeg::Info;
use glitchrx_core::recipe::with_restarts;
use glitchrx_core::{Jpeg, Mode, Recipe, Restart, Result, Step};

/// Upper bound on cached per-step payloads, in bytes, per document. A 24 MP
/// photo has roughly a 10 MB payload, so this keeps about nine steps.
const STEP_CACHE_BYTES: usize = 96 << 20;

/// One open photo.
pub struct Doc {
    pub id: String,
    /// File name shown in the UI and used to name downloads.
    pub name: String,
    /// Absolute path when the photo was opened from the server's `--root`
    /// (so results can be saved next to it); `None` for uploads.
    pub source: Option<PathBuf>,
    pub original: Arc<Vec<u8>>,
    /// Layout of the original file.
    pub info: Info,
    /// Bumped by every preview request; a request that finds it changed by
    /// the time it gets to run has been superseded and is dropped.
    generation: AtomicU64,
    cache: Mutex<RenderCache>,
}

#[derive(Default)]
struct RenderCache {
    base: Option<Base>,
}

/// The prepared, parsed input plus per-step payload snapshots.
struct Base {
    restart: Option<Restart>,
    mode: Mode,
    /// Parsed prepared input; its payload is overwritten for every render.
    jpeg: Jpeg,
    /// The payload before any step ran.
    pristine: Vec<u8>,
    /// `steps[i]` holds step `i` and the payload after steps `0..=i`.
    steps: Vec<(Step, Vec<u8>)>,
}

impl Doc {
    /// Parse `bytes` as a JPEG and wrap it as a document.
    pub fn new(id: String, name: String, source: Option<PathBuf>, bytes: Vec<u8>) -> Result<Doc> {
        let info = Jpeg::parse(bytes.clone(), Mode::Safe)?.info().clone();
        Ok(Doc {
            id,
            name,
            source,
            original: Arc::new(bytes),
            info,
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

    /// Glitch the original with `recipe`, reusing cached work.
    pub fn glitch(&self, recipe: &Recipe) -> Result<Vec<u8>> {
        // Validate every step before touching the cache.
        let prepared = recipe
            .steps
            .iter()
            .map(Step::prepare)
            .collect::<Result<Vec<_>>>()?;

        let mut cache = self.lock();
        let reuse =
            matches!(&cache.base, Some(b) if b.restart == recipe.restart && b.mode == recipe.mode);
        if !reuse {
            let input = match recipe.restart {
                Some(r) => with_restarts(self.original.to_vec(), r)?,
                None => self.original.to_vec(),
            };
            let jpeg = Jpeg::parse(input, recipe.mode)?;
            cache.base = Some(Base {
                restart: recipe.restart,
                mode: recipe.mode,
                pristine: jpeg.payload().to_vec(),
                jpeg,
                steps: Vec::new(),
            });
        }
        let base = cache.base.as_mut().expect("base was just set");

        // Longest prefix of steps that is already cached.
        let keep = base
            .steps
            .iter()
            .zip(&recipe.steps)
            .take_while(|((cached, _), step)| cached == *step)
            .count();
        base.steps.truncate(keep);

        let mut payload = match keep {
            0 => base.pristine.clone(),
            n => base.steps[n - 1].1.clone(),
        };
        let max_cached = (STEP_CACHE_BYTES / payload.len().max(1)).max(1);
        for (step, p) in recipe.steps[keep..].iter().zip(&prepared[keep..]) {
            p.apply(&mut payload);
            if base.steps.len() < max_cached {
                base.steps.push((step.clone(), payload.clone()));
            }
        }
        base.jpeg.payload_mut().copy_from_slice(&payload);
        Ok(base.jpeg.finish())
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

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/../../tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(path).unwrap()
    }

    fn recipe(steps: &[(&str, &str, u64)], restart: Option<Restart>) -> Recipe {
        Recipe {
            restart,
            steps: steps
                .iter()
                .map(|(e, p, s)| Step::new(e, p, *s).unwrap())
                .collect(),
            ..Recipe::default()
        }
    }

    /// Whatever order edits arrive in, the cached render equals a fresh
    /// `Recipe::apply`. This is the "preview is the export" guarantee.
    #[test]
    fn cached_render_matches_apply() {
        let bytes = fixture("baseline-420.jpg");
        let doc = Doc::new("t".into(), "t.jpg".into(), None, bytes.clone()).unwrap();
        let pct = Some(Restart::MilliPercent(10_000));
        let sequence = [
            recipe(&[("echo", "", 1)], pct),
            recipe(&[("echo", "", 1), ("shift", "", 2)], pct),
            recipe(&[("echo", "", 1), ("shift", "by=50", 2)], pct),
            recipe(
                &[("echo", "", 1), ("shift", "by=50", 2), ("stutter", "", 3)],
                pct,
            ),
            recipe(&[("echo", "gain=0.9", 1), ("shift", "by=50", 2)], pct),
            recipe(&[("echo", "gain=0.9", 1), ("shift", "by=50", 2)], None),
            recipe(&[], None),
            recipe(&[("backwards", "", 4)], Some(Restart::Rows(2))),
        ];
        for r in &sequence {
            assert_eq!(
                doc.glitch(r).unwrap(),
                r.apply(bytes.clone()).unwrap(),
                "{r:?}"
            );
        }
    }

    #[test]
    fn bad_step_is_an_error_not_a_panic() {
        let doc = Doc::new("t".into(), "t.jpg".into(), None, fixture("grayscale.jpg")).unwrap();
        let mut r = recipe(&[("echo", "", 1)], None);
        r.steps[0].params.insert("gain".into(), "7".into());
        assert!(doc.glitch(&r).is_err());
    }

    #[test]
    fn store_evicts_least_recently_used() {
        let store = Store::new(2);
        let make =
            |id: &str| Doc::new(id.into(), "x.jpg".into(), None, fixture("grayscale.jpg")).unwrap();
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
        let doc = Doc::new("t".into(), "t.jpg".into(), None, fixture("grayscale.jpg")).unwrap();
        let first = doc.begin();
        let second = doc.begin();
        assert!(!doc.is_current(first));
        assert!(doc.is_current(second));
    }

    #[test]
    fn random_hex_has_requested_length() {
        let a = random_hex(16);
        assert_eq!(a.len(), 32);
        assert_ne!(a, random_hex(16));
    }
}
