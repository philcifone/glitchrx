//! Property tests: for random rasters, parameters, encodings and preview
//! scales, every effect keeps the raster's length, writes only inside its
//! windows, is deterministic, and never panics.

use glitchrx_core::effects::REGISTRY;
use glitchrx_core::pull::Stroke as PullStroke;
use glitchrx_core::recipe::{NEVER, Recipe, Step};
use glitchrx_core::{Encoding, Units};
use proptest::prelude::*;

/// A size in one of the accepted units.
fn size() -> BoxedStrategy<String> {
    prop_oneof![
        (1u32..5000).prop_map(|b| b.to_string()),
        (1u32..400).prop_map(|p| format!("{p}px")),
        (1u32..4000).prop_map(|r| format!("{}rows", r as f64 / 1000.0)),
        (1u32..=100).prop_map(|p| format!("{p}%")),
    ]
    .boxed()
}

/// Random but valid parameter lists per effect.
fn params_for(effect: &'static str) -> BoxedStrategy<String> {
    let placement = (0u32..12, size(), 0u32..60, 40u32..=100)
        .prop_map(|(hits, len, a, b)| format!("hits={hits},len={len},region={a}%:{b}%"));
    let unit = || (0u32..=100).prop_map(|v| v as f64 / 100.0);
    let own: BoxedStrategy<String> = match effect {
        "echo" => (size(), 0u32..3, unit())
            .prop_map(|(d, p, g)| format!("delay={d},phase={p},decay={g}"))
            .boxed(),
        "delay" => (size(), 0u32..3, unit(), 1u32..8)
            .prop_map(|(d, p, g, t)| format!("delay={d},phase={p},decay={g},taps={t}"))
            .boxed(),
        "reverb" => (size(), unit(), unit(), unit(), unit())
            .prop_map(|(s, r, d, w, y)| format!("size={s},room={r},damping={d},wet={w},dry={y}"))
            .boxed(),
        "amplify" => (-40i32..40).prop_map(|g| format!("gain={g}")).boxed(),
        "bitcrush" => (1u32..=16).prop_map(|b| format!("bits={b}")).boxed(),
        "shift" => (size(), 0u32..3)
            .prop_map(|(b, p)| format!("by={b},phase={p}"))
            .boxed(),
        "shuffle" => (size(), unit())
            .prop_map(|(b, f)| format!("block={b},fraction={f}"))
            .boxed(),
        "stutter" => size().prop_map(|c| format!("chunk={c}")).boxed(),
        _ => Just(String::new()).boxed(),
    };
    (own, placement)
        .prop_map(|(o, p)| if o.is_empty() { p } else { format!("{o},{p}") })
        .boxed()
}

fn case() -> impl Strategy<Value = (usize, String, u64, usize, usize, usize, usize, usize)> {
    (0..REGISTRY.len())
        // A pull has no placement and no settings worth drawing here; it
        // has its own test below.
        .prop_filter("drawn by hand", |&i| !REGISTRY[i].strokes)
        .prop_flat_map(|i| {
            (
                Just(i),
                params_for(REGISTRY[i].id),
                any::<u64>(),
                0..Encoding::ALL.len(),
                1usize..=2,
                // Full size, and a preview no larger than it.
                (2usize..60, 2usize..40),
                (1usize..=4),
            )
        })
        .prop_map(|(i, p, seed, enc, ch, (w, h), div)| (i, p, seed, enc, ch, w, h, div))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn effects_are_safe((i, params, seed, enc, channels, w, h, div) in case()) {
        let step = Step::new(REGISTRY[i].id, &params, seed & i64::MAX as u64).unwrap();
        let recipe = Recipe {
            encoding: Encoding::ALL[enc],
            channels,
            steps: vec![step],
            ..Recipe::default()
        };
        let units = Units {
            full_width: w,
            full_height: h,
            width: (w / div).max(1),
            height: (h / div).max(1),
        };
        let original: Vec<u8> = (0..units.len()).map(|n| (n * 37 % 251) as u8).collect();
        let mut a = original.clone();
        recipe.apply_with(&mut a, &units, NEVER).unwrap();
        prop_assert_eq!(a.len(), original.len());

        let mut b = original.clone();
        recipe.apply_with(&mut b, &units, NEVER).unwrap();
        prop_assert_eq!(&a, &b, "not deterministic");

        let p = &recipe.prepare_all().unwrap()[0];
        let mut inside = vec![false; original.len()];
        for win in p.placement.windows(&units, p.frame(), p.seed) {
            prop_assert!(win.end <= original.len() && win.start < win.end);
            prop_assert_eq!(win.start % p.frame(), 0);
            prop_assert_eq!(win.len() % p.frame(), 0);
            inside[win].iter_mut().for_each(|x| *x = true);
        }
        for (n, (x, y)) in original.iter().zip(&a).enumerate() {
            prop_assert!(inside[n] || x == y, "byte {} changed outside the windows", n);
        }
    }
}

/// Random strokes: any size, strength and path (partly off the picture),
/// on any canvas and preview size. A pull never panics, keeps the length,
/// is deterministic, and with `pixels` only copies pixels of its input.
fn strokes() -> impl Strategy<Value = Vec<PullStroke>> {
    prop::collection::vec(
        (
            // Brushes up to a third of the picture keep a debug build quick;
            // the size only scales the work, not what can go wrong.
            0.001f64..=0.35,
            0.0f64..=1.0,
            prop::collection::vec(-0.5f64..1.5, 1..8),
        )
            .prop_map(|(size, strength, mut points)| {
                if points.len() % 2 == 1 {
                    points.pop();
                }
                if points.is_empty() {
                    points = vec![0.5, 0.5];
                }
                PullStroke {
                    size,
                    strength,
                    points,
                }
            }),
        0..4,
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn pulls_are_safe(
        strokes in strokes(),
        smooth in any::<bool>(),
        (w, h) in (1usize..80, 1usize..60),
        div in 1usize..=4,
    ) {
        let mut step = Step::new("pull", if smooth { "sampling=smooth" } else { "" }, 0).unwrap();
        step.pull = strokes;
        let recipe = Recipe { steps: vec![step], ..Recipe::default() };
        let units = Units {
            full_width: w,
            full_height: h,
            width: (w / div).max(1),
            height: (h / div).max(1),
        };
        let original: Vec<u8> = (0..units.len()).map(|n| (n * 37 % 251) as u8).collect();
        let mut a = original.clone();
        recipe.apply_with(&mut a, &units, NEVER).unwrap();
        prop_assert_eq!(a.len(), original.len());
        let mut b = original.clone();
        recipe.apply_with(&mut b, &units, NEVER).unwrap();
        prop_assert_eq!(&a, &b, "not deterministic");
        if !smooth {
            let pixels: std::collections::HashSet<_> = original.chunks(3).collect();
            prop_assert!(a.chunks(3).all(|p| pixels.contains(p)));
        }
    }
}
