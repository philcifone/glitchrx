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
    /// A length: bytes (`4096`, `4k`, `1m`) or percent of the payload (`2%`).
    Size,
    /// A `START:END` pair of positions, each bytes or percent.
    Region,
    /// One word out of a fixed list.
    Choice(&'static [&'static str]),
}

/// Declaration of one parameter.
#[derive(Debug, Clone, Copy)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    pub help: &'static str,
}

/// A position or length within the payload: absolute bytes or a fraction.
///
/// Percentages are stored as integer milli-percent (1% = 1000) so that
/// resolving them against a length is pure integer math, identical on every
/// platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Bytes(u64),
    MilliPercent(u32),
}

impl Bound {
    /// Resolve against a payload length, clamped to `len`.
    pub fn resolve(self, len: usize) -> usize {
        let v = match self {
            Bound::Bytes(b) => b.min(len as u64) as usize,
            Bound::MilliPercent(mp) => ((len as u128 * mp as u128) / 100_000) as usize,
        };
        v.min(len)
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
        .map_err(|_| format!("`{s}` is not a byte count or percentage"))?;
    Ok(Bound::Bytes(n.saturating_mul(mult)))
}

/// Convert a decimal to Q8 fixed point (x * 256, rounded).
pub fn to_q8(v: f64) -> i32 {
    (v * 256.0).round() as i32
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

    /// A float parameter as Q8 fixed point.
    pub fn q8(&self, name: &str) -> i32 {
        to_q8(self.float(name))
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
            if matches!(b, Bound::Bytes(0) | Bound::MilliPercent(0)) {
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds() {
        assert_eq!(parse_bound("4k").unwrap(), Bound::Bytes(4096));
        assert_eq!(parse_bound("1M").unwrap(), Bound::Bytes(1 << 20));
        assert_eq!(parse_bound("12.5%").unwrap(), Bound::MilliPercent(12_500));
        assert!(parse_bound("101%").is_err());
        assert!(parse_bound("abc").is_err());
        assert_eq!(Bound::MilliPercent(50_000).resolve(1001), 500);
        assert_eq!(Bound::Bytes(5000).resolve(100), 100);
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
