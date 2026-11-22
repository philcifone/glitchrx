//! The raster: a decoded image as raw bytes, the thing every effect edits.
//!
//! Layout: 8-bit R, G, B for each pixel, pixels left to right, rows top to
//! bottom, no padding. This is exactly what an uncompressed RGB TIFF stores,
//! which is what Audacity saw when Phil imported TIFFs as raw audio.

/// An 8-bit RGB image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: usize,
    pub height: usize,
    /// `width * height * 3` bytes.
    pub bytes: Vec<u8>,
}

impl Raster {
    pub fn new(width: usize, height: usize, bytes: Vec<u8>) -> Raster {
        assert_eq!(bytes.len(), width * height * 3, "raster size mismatch");
        Raster {
            width,
            height,
            bytes,
        }
    }

    /// Bytes per row.
    pub fn row_bytes(&self) -> usize {
        self.width * 3
    }

    /// Apply an EXIF orientation (1-8) so the raster is upright.
    ///
    /// Cameras store a sideways photo as-is and set a tag saying how to turn
    /// it. Effects work along rows, so the raster must be upright first or
    /// "rows" would run down the picture.
    pub fn orient(self, orientation: u8) -> Raster {
        if !(2..=8).contains(&orientation) {
            return self;
        }
        let (w, h) = (self.width, self.height);
        let swap = orientation >= 5;
        let (ow, oh) = if swap { (h, w) } else { (w, h) };
        let mut out = vec![0u8; self.bytes.len()];
        for oy in 0..oh {
            for ox in 0..ow {
                // Where display pixel (ox, oy) comes from in the stored image.
                let (sx, sy) = match orientation {
                    2 => (w - 1 - ox, oy),
                    3 => (w - 1 - ox, h - 1 - oy),
                    4 => (ox, h - 1 - oy),
                    5 => (oy, ox),
                    6 => (oy, h - 1 - ox),
                    7 => (w - 1 - oy, h - 1 - ox),
                    _ => (w - 1 - oy, ox), // 8
                };
                let s = (sy * w + sx) * 3;
                let d = (oy * ow + ox) * 3;
                out[d..d + 3].copy_from_slice(&self.bytes[s..s + 3]);
            }
        }
        Raster::new(ow, oh, out)
    }

    /// A copy of the `w` x `h` rectangle whose top-left pixel is `(x, y)`.
    /// The rectangle is clipped to the raster, and is at least 1 x 1. Used
    /// for zoomed previews, which show part of a full-resolution render.
    pub fn crop(&self, x: usize, y: usize, w: usize, h: usize) -> Raster {
        let x = x.min(self.width.saturating_sub(1));
        let y = y.min(self.height.saturating_sub(1));
        let w = w.clamp(1, self.width - x);
        let h = h.clamp(1, self.height - y);
        let row = self.row_bytes();
        let mut bytes = Vec::with_capacity(w * h * 3);
        for r in y..y + h {
            let start = r * row + x * 3;
            bytes.extend_from_slice(&self.bytes[start..start + w * 3]);
        }
        Raster::new(w, h, bytes)
    }

    /// The size that fits inside `max_w` x `max_h` keeping the aspect ratio
    /// (never larger than the raster itself).
    pub fn fit(&self, max_w: usize, max_h: usize) -> (usize, usize) {
        let (w, h) = (self.width.max(1), self.height.max(1));
        if w <= max_w && h <= max_h {
            return (w, h);
        }
        // Compare w/max_w with h/max_h without floats.
        if w * max_h >= h * max_w {
            (max_w.max(1), (h * max_w / w).max(1))
        } else {
            ((w * max_h / h).max(1), max_h.max(1))
        }
    }

    /// Downscale to fit `max_w` x `max_h` (see [`Raster::fit`]).
    pub fn downscale(&self, max_w: usize, max_h: usize) -> Raster {
        let (w, h) = self.fit(max_w, max_h);
        self.resize(w, h)
    }

    /// Shrink to exactly `ow` x `oh` (each no larger than the raster) by
    /// averaging the source pixels that cover each output pixel (a box
    /// filter). Good enough for previews and thumbnails, and fast.
    ///
    /// Callers that already chose a size must use this rather than
    /// [`Raster::downscale`]: `fit` is not idempotent (fitting 480x320 into
    /// 200x200 gives 200x133, but fitting it into 200x133 gives 199x133,
    /// since the rounding flips which side limits).
    pub fn resize(&self, ow: usize, oh: usize) -> Raster {
        let (ow, oh) = (
            ow.clamp(1, self.width.max(1)),
            oh.clamp(1, self.height.max(1)),
        );
        if (ow, oh) == (self.width, self.height) {
            return self.clone();
        }
        let mut out = vec![0u8; ow * oh * 3];
        let mut acc = vec![0u32; ow * 3];
        let mut counts = vec![0u32; ow];
        // Source column range for each output column, computed once.
        let cols: Vec<(usize, usize)> = (0..ow)
            .map(|ox| {
                let a = ox * self.width / ow;
                let b = ((ox + 1) * self.width / ow).max(a + 1);
                (a, b)
            })
            .collect();
        for oy in 0..oh {
            let y0 = oy * self.height / oh;
            let y1 = ((oy + 1) * self.height / oh).max(y0 + 1);
            acc.iter_mut().for_each(|v| *v = 0);
            counts.iter_mut().for_each(|v| *v = 0);
            for sy in y0..y1 {
                let row = &self.bytes[sy * self.width * 3..(sy + 1) * self.width * 3];
                for (ox, &(a, b)) in cols.iter().enumerate() {
                    for px in row[a * 3..b * 3].chunks_exact(3) {
                        acc[ox * 3] += px[0] as u32;
                        acc[ox * 3 + 1] += px[1] as u32;
                        acc[ox * 3 + 2] += px[2] as u32;
                    }
                    counts[ox] += (b - a) as u32;
                }
            }
            let dst = &mut out[oy * ow * 3..(oy + 1) * ow * 3];
            for ox in 0..ow {
                let n = counts[ox];
                for c in 0..3 {
                    dst[ox * 3 + c] = ((acc[ox * 3 + c] + n / 2) / n) as u8;
                }
            }
        }
        Raster::new(ow, oh, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 3x2 raster whose pixels are numbered 0..6 (in the red channel).
    fn numbered() -> Raster {
        let bytes = (0..6u8).flat_map(|i| [i, 0, 0]).collect();
        Raster::new(3, 2, bytes)
    }

    fn reds(r: &Raster) -> Vec<u8> {
        r.bytes.chunks_exact(3).map(|p| p[0]).collect()
    }

    #[test]
    fn crop_takes_a_rectangle_and_clips_it() {
        let r = numbered();
        assert_eq!(reds(&r.crop(1, 0, 2, 2)), [1, 2, 4, 5]);
        assert_eq!(reds(&r.crop(0, 1, 3, 1)), [3, 4, 5]);
        // Past the edges: clipped, never empty.
        let c = r.crop(2, 1, 9, 9);
        assert_eq!((c.width, c.height, reds(&c)), (1, 1, vec![5]));
        let c = r.crop(7, 7, 0, 0);
        assert_eq!((c.width, c.height, reds(&c)), (1, 1, vec![5]));
        assert_eq!(r.crop(0, 0, 3, 2), r);
    }

    /// Stored:   0 1 2
    ///           3 4 5
    #[test]
    fn orientations() {
        let cases: [(u8, (usize, usize), [u8; 6]); 8] = [
            (1, (3, 2), [0, 1, 2, 3, 4, 5]),
            (2, (3, 2), [2, 1, 0, 5, 4, 3]),
            (3, (3, 2), [5, 4, 3, 2, 1, 0]),
            (4, (3, 2), [3, 4, 5, 0, 1, 2]),
            (5, (2, 3), [0, 3, 1, 4, 2, 5]),
            (6, (2, 3), [3, 0, 4, 1, 5, 2]),
            (7, (2, 3), [5, 2, 4, 1, 3, 0]),
            (8, (2, 3), [2, 5, 1, 4, 0, 3]),
        ];
        for (o, dims, want) in cases {
            let r = numbered().orient(o);
            assert_eq!((r.width, r.height), dims, "orientation {o}");
            assert_eq!(reds(&r), want, "orientation {o}");
        }
    }

    #[test]
    fn downscale_averages_and_keeps_aspect() {
        let r = Raster::new(
            4,
            2,
            [10u8, 20, 30, 50]
                .iter()
                .flat_map(|&v| [v; 3])
                .cycle()
                .take(24)
                .collect(),
        );
        let d = r.downscale(2, 2);
        assert_eq!((d.width, d.height), (2, 1));
        assert_eq!(reds(&d), vec![15, 40]);
        assert_eq!(r.fit(100, 100), (4, 2), "never upscales");
        assert_eq!(
            Raster::new(1000, 10, vec![0; 30_000]).fit(100, 100),
            (100, 1)
        );
    }
}
