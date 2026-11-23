//! Effect parameters: declaration, parsing and validation.
//!
//! Parameters travel as text (`delay=4096,gain=0.7` on the command line, or
//! typed TOML values that are converted to text) and are parsed once, against
//! the effect's declared [`ParamSpec`]s, before any bytes are touched. That
//! way a typo fails fast with a message naming the valid options.

use std::collections::BTreeMap;

use crate::error::{Error, Result};

/// What kind of value a parameter accepts.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// Whole number in an inclusive range.
    Int { min: i64, max: i64 },
    /// Decimal number in an inclusive range. Quantized to Q8 for the math.
    Float { min: f64, max: f64 },
    /// A length: bytes (`4096`, `4k`), pixels (`300px`), image rows
    /// (`2.5rows`, where a fractional row is that share of the width), or
    /// percent (`2%`; of the image, or of the window where the effect says so).
    Size,
    /// A `START:END` pair of positions, each in any [`Kind::Size`] unit.
    Region,
    /// One word out of a fixed list.
    Choice(&'static [&'static str]),
    /// A tone curve: points `IN:OUT` in 0..=255, joined by `/`, inputs
    /// rising (`0:0/64:40/192:215/255:255`). `/` rather than `,` because
    /// the command line separates parameters with commas.
    Curve,
}

/// The most points a curve may have.
pub const MAX_CURVE_POINTS: usize = 16;

/// Declaration of one parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    pub help: &'static str,
}

/// A position, length or distance in the raster, in the unit it was given.
///
/// Percentages and rows are stored as integer thousandths (1% = 1000,
/// 1 row = 1000) so that resolving them is integer math, identical on every
/// platform. How a bound turns into bytes depends on what it measures; see
/// [`Units`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Bytes(u64),
    Pixels(u64),
    MilliRows(u64),
    MilliPercent(u32),
}

impl Bound {
    /// A size relative to a window (`shuffle` block, `stutter` chunk):
    /// percent is of `reference` bytes, every other unit is a length in the
    /// picture. Clamped to `reference`.
    pub fn of_window(self, reference: usize, units: &Units) -> usize {
        match self {
            Bound::MilliPercent(mp) => ((reference as u128 * mp as u128) / 100_000) as usize,
            other => units.to_raster(units.full_bytes(other)),
        }
        .min(reference)
    }
}

/// Converts bounds to byte counts for one raster.
///
/// Live previews run the recipe on a screen-sized copy of the image (SPEC
/// D-26). For the preview to look like the export, every size and position
/// must mean the same *place in the picture* at both resolutions. So sizes
/// are always worked out at full resolution first ([`Units::full_bytes`])
/// and then mapped into the raster being processed ([`Units::to_raster`]):
///
/// - whole rows scale with the height, the rest of a row with the width, so
///   "2 rows and 300 px" at full size is "2/6 rows and 50 px" at 1/6 size;
/// - the remainder of 0-2 bytes (the channel `phase`, which rotates colours)
///   is kept as is.
///
/// Scaling bytes in proportion to the image's area instead would be wrong
/// twice over: a third of a row at full size would become a twelfth of one
/// at 1/4 size, and the byte remainder (the colour rotation) would come out
/// arbitrary.
///
/// At full resolution (`width == full_width`) the mapping is the identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Units {
    /// Width in pixels of the image as decoded (export resolution).
    pub full_width: usize,
    pub full_height: usize,
    /// Size of the raster being processed (smaller for previews).
    pub width: usize,
    pub height: usize,
}

impl Units {
    /// Units for a raster processed at its own (full) resolution.
    pub fn full(width: usize, height: usize) -> Units {
        Units {
            full_width: width,
            full_height: height,
            width,
            height,
        }
    }

    /// Bytes in the full-resolution raster.
    pub fn full_len(&self) -> usize {
        self.full_width * self.full_height * 3
    }

    /// Bytes in the raster being processed.
    pub fn len(&self) -> usize {
        self.width * self.height * 3
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn scaled(&self) -> bool {
        (self.width, self.height) != (self.full_width, self.full_height)
    }

    /// A bound as full-resolution bytes. Pixels, rows and percent land on
    /// whole pixels; plain bytes are kept exactly.
    pub fn full_bytes(&self, b: Bound) -> usize {
        match b {
            Bound::Bytes(n) => n as usize,
            Bound::Pixels(p) => p as usize * 3,
            Bound::MilliRows(mr) => {
                ((self.full_width as u128 * mr as u128 + 500) / 1000) as usize * 3
            }
            Bound::MilliPercent(mp) => {
                ((self.full_len() as u128 * mp as u128) / 100_000) as usize / 3 * 3
            }
        }
    }

    /// Map a full-resolution byte distance (or position) into the raster
    /// being processed: rows by the height ratio, the rest of a row by the
    /// width ratio, and the byte remainder unchanged.
    pub fn to_raster(&self, full: usize) -> usize {
        if !self.scaled() {
            return full;
        }
        let row = (self.full_width * 3).max(1);
        let (rows, rest) = (full / row, full % row);
        let (px, keep) = (rest / 3, rest % 3);
        let ratio = |v: usize, num: usize, den: usize| (v * num + den / 2) / den.max(1);
        let rows = ratio(rows, self.height, self.full_height);
        let px = ratio(px, self.width, self.full_width).min(self.width.saturating_sub(1));
        (rows * self.width + px) * 3 + keep
    }

    /// A position or length along the raster, in bytes (clamped to it).
    pub fn span(&self, b: Bound) -> usize {
        self.to_raster(self.full_bytes(b).min(self.full_len()))
            .min(self.len())
    }

    /// A displacement in bytes, plus `phase` extra bytes.
    pub fn offset(&self, b: Bound, phase: usize) -> usize {
        self.to_raster(self.full_bytes(b) + phase)
    }
}

/// Parse `"12"`, `"4k"`, `"1m"`, `"12.5%"`.
fn parse_bound(s: &str) -> std::result::Result<Bound, String> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        let v: f64 = p
            .trim()
            .parse()
            .map_err(|_| format!("`{s}` is not a percentage"))?;
        if !(0.0..=100.0).contains(&v) {
            return Err(format!("`{s}` must be between 0% and 100%"));
        }
        // Parsing a decimal string to f64 is correctly rounded (IEEE), and the
        // product below is rounded once, so this is deterministic.
        return Ok(Bound::MilliPercent((v * 1000.0).round() as u32));
    }
    let lower = s.to_ascii_lowercase();
    if let Some(r) = lower
        .strip_suffix("rows")
        .or_else(|| lower.strip_suffix("row"))
    {
        let v: f64 = r
            .trim()
            .parse()
            .map_err(|_| format!("`{s}` is not a number of rows"))?;
        if !(0.0..=1e6).contains(&v) {
            return Err(format!("`{s}` is out of range"));
        }
        return Ok(Bound::MilliRows((v * 1000.0).round() as u64));
    }
    if let Some(p) = lower.strip_suffix("px") {
        let v: u64 = p
            .trim()
            .parse()
            .map_err(|_| format!("`{s}` is not a whole number of pixels"))?;
        return Ok(Bound::Pixels(v));
    }
    let (digits, mult) = if let Some(d) = lower.strip_suffix('k') {
        (d, 1024u64)
    } else if let Some(d) = lower.strip_suffix('m') {
        (d, 1024 * 1024)
    } else {
        (lower.as_str(), 1)
    };
    let n: u64 = digits
        .trim()
        .parse()
        .map_err(|_| format!("`{s}` is not a size (try 4096, 300px, 2.5rows or 10%)"))?;
    Ok(Bound::Bytes(n.saturating_mul(mult)))
}

/// Parse a `key=value,key=value` list into a map. Empty input gives an
/// empty map. Keys are case-sensitive and must not repeat.
pub fn parse_list(effect: &str, s: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (k, v) = part
            .split_once('=')
            .ok_or_else(|| Error::param(effect, format!("expected key=value, got `{part}`")))?;
        let k = k.trim().to_string();
        if out.insert(k.clone(), v.trim().to_string()).is_some() {
            return Err(Error::param(effect, format!("`{k}` given twice")));
        }
    }
    Ok(out)
}

/// A parsed value, after validation against its spec.
#[derive(Debug, Clone)]
enum Value {
    Int(i64),
    Float(f64),
    Size(Bound),
    Region(Bound, Bound),
    Choice(String),
    Curve(Vec<(u8, u8)>),
}

/// Validated parameters for one effect instance, with defaults filled in.
#[derive(Debug, Clone)]
pub struct Params {
    effect: String,
    values: BTreeMap<&'static str, Value>,
}

impl Params {
    /// Validate user-supplied text values against `specs`.
    ///
    /// Unknown keys are rejected; missing keys take the spec default.
    pub fn new(
        effect: &str,
        specs: &[ParamSpec],
        given: &BTreeMap<String, String>,
    ) -> Result<Params> {
        for key in given.keys() {
            if !specs.iter().any(|s| s.name == key) {
                let valid: Vec<&str> = specs.iter().map(|s| s.name).collect();
                return Err(Error::param(
                    effect,
                    format!("unknown parameter `{key}` (valid: {})", valid.join(", ")),
                ));
            }
        }
        let mut values = BTreeMap::new();
        for spec in specs {
            let text = given
                .get(spec.name)
                .map(String::as_str)
                .unwrap_or(spec.default);
            let v = parse_value(spec, text)
                .map_err(|m| Error::param(effect, format!("{}: {m}", spec.name)))?;
            values.insert(spec.name, v);
        }
        Ok(Params {
            effect: effect.to_string(),
            values,
        })
    }

    /// The effect these parameters belong to (for error messages).
    pub fn effect(&self) -> &str {
        &self.effect
    }

    fn get(&self, name: &str) -> &Value {
        self.values
            .get(name)
            .unwrap_or_else(|| panic!("effect {} has no parameter {name}", self.effect))
    }

    pub fn int(&self, name: &str) -> i64 {
        match self.get(name) {
            Value::Int(v) => *v,
            other => panic!("{name} is not an int: {other:?}"),
        }
    }

    pub fn float(&self, name: &str) -> f64 {
        match self.get(name) {
            Value::Float(v) => *v,
            other => panic!("{name} is not a float: {other:?}"),
        }
    }

    pub fn size(&self, name: &str) -> Bound {
        match self.get(name) {
            Value::Size(v) => *v,
            other => panic!("{name} is not a size: {other:?}"),
        }
    }

    pub fn region(&self, name: &str) -> (Bound, Bound) {
        match self.get(name) {
            Value::Region(a, b) => (*a, *b),
            other => panic!("{name} is not a region: {other:?}"),
        }
    }

    pub fn choice(&self, name: &str) -> &str {
        match self.get(name) {
            Value::Choice(v) => v,
            other => panic!("{name} is not a choice: {other:?}"),
        }
    }

    /// A curve's points, inputs rising.
    pub fn curve(&self, name: &str) -> &[(u8, u8)] {
        match self.get(name) {
            Value::Curve(v) => v,
            other => panic!("{name} is not a curve: {other:?}"),
        }
    }
}

fn parse_value(spec: &ParamSpec, text: &str) -> std::result::Result<Value, String> {
    let t = text.trim();
    match spec.kind {
        Kind::Int { min, max } => {
            let v: i64 = t
                .parse()
                .map_err(|_| format!("`{t}` is not a whole number"))?;
            if v < min || v > max {
                return Err(format!("{v} is outside {min}..={max}"));
            }
            Ok(Value::Int(v))
        }
        Kind::Float { min, max } => {
            let v: f64 = t.parse().map_err(|_| format!("`{t}` is not a number"))?;
            if !v.is_finite() || v < min || v > max {
                return Err(format!("{t} is outside {min}..={max}"));
            }
            Ok(Value::Float(v))
        }
        Kind::Size => {
            let b = parse_bound(t)?;
            if matches!(
                b,
                Bound::Bytes(0) | Bound::Pixels(0) | Bound::MilliRows(0) | Bound::MilliPercent(0)
            ) {
                return Err("must be greater than zero".into());
            }
            Ok(Value::Size(b))
        }
        Kind::Region => {
            let (a, b) = t
                .split_once(':')
                .ok_or_else(|| format!("`{t}` should look like START:END, e.g. 5%:100%"))?;
            let a = if a.trim().is_empty() {
                Bound::Bytes(0)
            } else {
                parse_bound(a)?
            };
            let b = if b.trim().is_empty() {
                Bound::MilliPercent(100_000)
            } else {
                parse_bound(b)?
            };
            Ok(Value::Region(a, b))
        }
        Kind::Choice(options) => {
            if options.contains(&t) {
                Ok(Value::Choice(t.to_string()))
            } else {
                Err(format!("`{t}` is not one of: {}", options.join(", ")))
            }
        }
        Kind::Curve => parse_curve(t).map(Value::Curve),
    }
}

/// `IN:OUT/IN:OUT/...`, each 0..=255, at least two points, inputs rising.
fn parse_curve(t: &str) -> std::result::Result<Vec<(u8, u8)>, String> {
    let shape = "should look like 0:0/128:150/255:255 (IN:OUT pairs, 0 to 255, inputs rising)";
    let points = t
        .split('/')
        .map(|p| {
            let (a, b) = p.split_once(':').ok_or_else(|| format!("`{t}` {shape}"))?;
            let n = |v: &str| v.trim().parse::<u8>().map_err(|_| format!("`{t}` {shape}"));
            Ok((n(a)?, n(b)?))
        })
        .collect::<std::result::Result<Vec<_>, String>>()?;
    if points.len() < 2 || points.len() > MAX_CURVE_POINTS {
        return Err(format!(
            "a curve has 2 to {MAX_CURVE_POINTS} points, not {}",
            points.len()
        ));
    }
    if points.windows(2).any(|w| w[1].0 <= w[0].0) {
        return Err(format!("`{t}` {shape}"));
    }
    Ok(points)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curves() {
        assert_eq!(
            parse_curve("0:0/128:150/255:255").unwrap(),
            vec![(0, 0), (128, 150), (255, 255)]
        );
        assert_eq!(
            parse_curve(" 0 : 10 / 255 : 200 ").unwrap(),
            vec![(0, 10), (255, 200)]
        );
        for bad in [
            "0:0",
            "0:0/0:5",
            "10:0/5:5",
            "0:0/256:1",
            "0:0,255:255",
            "a:b/1:1",
            "",
        ] {
            assert!(parse_curve(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn bounds() {
        assert_eq!(parse_bound("4k").unwrap(), Bound::Bytes(4096));
        assert_eq!(parse_bound("1M").unwrap(), Bound::Bytes(1 << 20));
        assert_eq!(parse_bound("12.5%").unwrap(), Bound::MilliPercent(12_500));
        assert!(parse_bound("101%").is_err());
        assert!(parse_bound("abc").is_err());
        assert_eq!(parse_bound("300px").unwrap(), Bound::Pixels(300));
        assert_eq!(parse_bound("2.5rows").unwrap(), Bound::MilliRows(2500));
        assert_eq!(parse_bound("1 row").unwrap(), Bound::MilliRows(1000));
        assert!(parse_bound("1.5px").is_err());
    }

    #[test]
    fn full_resolution_units_are_exact() {
        let u = Units::full(100, 50); // 300-byte rows, 15000 bytes
        assert_eq!(u.span(Bound::MilliPercent(50_000)), 7500);
        assert_eq!(u.span(Bound::Bytes(99_999)), 15_000, "clamped");
        assert_eq!(u.span(Bound::MilliRows(2000)), 600);
        assert_eq!(u.offset(Bound::MilliRows(330), 1), 33 * 3 + 1);
        assert_eq!(u.offset(Bound::Pixels(7), 2), 23);
        assert_eq!(u.offset(Bound::Bytes(4097), 0), 4097);
        // Percent offsets land on whole pixels.
        assert_eq!(u.offset(Bound::MilliPercent(1), 0) % 3, 0);
        assert_eq!(Bound::MilliPercent(25_000).of_window(400, &u), 100);
        assert_eq!(Bound::MilliRows(1000).of_window(400, &u), 300);
    }

    #[test]
    fn scaled_units_keep_geometry() {
        // Full 600x400, preview 100x67 (1/6).
        let u = Units {
            full_width: 600,
            full_height: 400,
            width: 100,
            height: 67,
        };
        // 1/3 row right becomes 1/3 of the preview row, phase kept.
        assert_eq!(u.offset(Bound::MilliRows(333), 1), 33 * 3 + 1);
        // 12 rows and 60 px down-right becomes 2 rows and 10 px.
        assert_eq!(
            u.offset(Bound::Pixels(12 * 600 + 60), 0),
            (2 * 100 + 10) * 3
        );
        // Bytes keep their remainder mod 3.
        assert_eq!(u.offset(Bound::Bytes(3 * 60 + 2), 0), 10 * 3 + 2);
        // Half the image is (about) half the preview's rows.
        assert_eq!(u.span(Bound::MilliPercent(50_000)), 34 * 100 * 3);
        // The whole image is the whole preview.
        assert_eq!(u.span(Bound::MilliPercent(100_000)), 100 * 67 * 3);
    }

    #[test]
    fn list_parsing() {
        let m = parse_list("echo", "delay=10, gain=0.5").unwrap();
        assert_eq!(m["delay"], "10");
        assert_eq!(m["gain"], "0.5");
        assert!(parse_list("echo", "delay").is_err());
        assert!(parse_list("echo", "a=1,a=2").is_err());
        assert!(parse_list("echo", "").unwrap().is_empty());
    }

    #[test]
    fn validation() {
        const SPECS: &[ParamSpec] = &[ParamSpec {
            name: "n",
            kind: Kind::Int { min: 1, max: 9 },
            default: "3",
            help: "",
        }];
        let p = Params::new("x", SPECS, &BTreeMap::new()).unwrap();
        assert_eq!(p.int("n"), 3);
        let bad = parse_list("x", "n=10").unwrap();
        assert!(Params::new("x", SPECS, &bad).is_err());
        let unknown = parse_list("x", "m=1").unwrap();
        let err = Params::new("x", SPECS, &unknown).unwrap_err().to_string();
        assert!(err.contains("valid: n"), "{err}");
    }
}
