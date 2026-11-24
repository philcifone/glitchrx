//! Download links: finished exports held in memory for a few minutes under
//! an unguessable address, so the browser can fetch them as an ordinary
//! file with a real name.
//!
//! Why not hand the browser the bytes it already fetched (a `blob:` URL)?
//! Firefox on iOS saves those under the blob's random id
//! (`b559de61-...`) and ignores the name the page asks for. A plain GET with
//! `Content-Disposition` is named correctly everywhere.
//!
//! The link itself is the credential: 128 random bits, alive for
//! [`LIFETIME`]. Download managers that fetch the file outside the page
//! (and so without its cookie) still get it, and nothing else on the
//! server is reachable that way.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::docs::random_hex;

/// How long a link works. Long enough for a slow phone to start the
/// download, short enough that exports do not pile up in memory.
pub const LIFETIME: Duration = Duration::from_secs(10 * 60);
/// At most this many files are held; the oldest goes first.
const MAX_FILES: usize = 6;
/// And at most this many bytes in all (a 50 MP TIFF is about 150 MB).
const MAX_BYTES: usize = 768 << 20;

/// One file waiting to be fetched.
#[derive(Clone)]
pub struct Download {
    pub name: String,
    pub mime: String,
    pub bytes: Arc<Vec<u8>>,
}

struct Entry {
    id: String,
    made: Instant,
    file: Download,
}

#[derive(Default)]
pub struct Downloads {
    items: Mutex<Vec<Entry>>,
}

impl Downloads {
    /// Hold `bytes` and return the id of its link.
    pub fn put(&self, name: String, mime: String, bytes: Vec<u8>) -> String {
        let id = random_hex(16);
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        prune(&mut items);
        items.push(Entry {
            id: id.clone(),
            made: Instant::now(),
            file: Download {
                name,
                mime,
                bytes: Arc::new(bytes),
            },
        });
        // Over the limits: drop the oldest, but never the one just added.
        let total = |v: &Vec<Entry>| v.iter().map(|e| e.file.bytes.len()).sum::<usize>();
        while items.len() > 1 && (items.len() > MAX_FILES || total(&items) > MAX_BYTES) {
            items.remove(0);
        }
        id
    }

    /// The file behind a live link. Not removed on the first fetch: some
    /// download managers ask twice (once to look, once to save).
    pub fn get(&self, id: &str) -> Option<Download> {
        let mut items = self.items.lock().unwrap_or_else(|e| e.into_inner());
        prune(&mut items);
        items
            .iter()
            .find(|e| crate::api::same_secret(&e.id, id))
            .map(|e| e.file.clone())
    }
}

fn prune(items: &mut Vec<Entry>) {
    items.retain(|e| e.made.elapsed() < LIFETIME);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_files_under_random_ids() {
        let d = Downloads::default();
        let a = d.put("a.png".into(), "image/png".into(), vec![1, 2, 3]);
        let b = d.put("b.png".into(), "image/png".into(), vec![4]);
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert_eq!(d.get(&a).unwrap().name, "a.png");
        assert_eq!(*d.get(&b).unwrap().bytes, vec![4]);
        // A second fetch still works.
        assert!(d.get(&a).is_some());
        assert!(d.get("0123456789abcdef0123456789abcdef").is_none());
    }

    #[test]
    fn keeps_only_the_newest() {
        let d = Downloads::default();
        let ids: Vec<String> = (0..MAX_FILES + 2)
            .map(|i| d.put(format!("{i}.png"), "image/png".into(), vec![0]))
            .collect();
        assert!(d.get(&ids[0]).is_none());
        assert!(d.get(&ids[1]).is_none());
        assert!(d.get(ids.last().unwrap()).is_some());
    }
}
