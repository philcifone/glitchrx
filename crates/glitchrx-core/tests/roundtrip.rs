//! Property-based proof that every reversible effect is exactly invertible,
//! for random payloads, parameters, seeds and hit layouts.

use glitchrx_core::effects::REGISTRY;
use glitchrx_core::recipe::Step;
use proptest::prelude::*;

/// Random but valid parameter lists per effect.
fn params_for(effect: &'static str) -> BoxedStrategy<String> {
    let placement = (0u32..20, 1u32..3000, 0u32..50)
        .prop_map(|(hits, len, start)| format!("hits={hits},len={len},region={start}%:100%"));
    let own: BoxedStrategy<String> = match effect {
        "echo" => (1u32..5000, 0u32..=100, 1u32..=8)
            .prop_map(|(d, g, t)| format!("delay={d},gain={},taps={t}", g as f64 / 100.0))
            .boxed(),
        "reverb" => (1u32..5000, 0u32..=100)
            .prop_map(|(d, f)| format!("delay={d},feedback={}", f as f64 / 100.0))
            .boxed(),
        "shift" => (-100_000i64..100_000)
            .prop_map(|b| format!("by={b}"))
            .boxed(),
        "shuffle" => (1u32..600, 0u32..=100)
            .prop_map(|(b, f)| format!("block={b},fraction={}", f as f64 / 100.0))
            .boxed(),
        "amplify" => (-60i32..60)
            .prop_map(|k| format!("factor={}", 2 * k + 1))
            .boxed(),
        _ => Just(String::new()).boxed(),
    };
    (own, placement)
        .prop_map(|(a, b)| if a.is_empty() { b } else { format!("{a},{b}") })
        .boxed()
}

fn case() -> impl Strategy<Value = (&'static str, String, u64, Vec<u8>)> {
    let reversible: Vec<&'static str> = REGISTRY
        .iter()
        .filter(|e| !matches!(e.id, "bitcrush" | "stutter"))
        .map(|e| e.id)
        .collect();
    proptest::sample::select(reversible).prop_flat_map(|id| {
        (
            Just(id),
            params_for(id),
            any::<u64>(),
            proptest::collection::vec(any::<u8>(), 1..20_000),
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn invert_undoes_apply((effect, params, seed, payload) in case()) {
        let step = Step::new(effect, &params, seed & glitchrx_core::rng::MAX_SEED).unwrap();
        let prepared = step.prepare().unwrap();
        prop_assert!(prepared.effect.reversible());
        let mut buf = payload.clone();
        prepared.apply(&mut buf);
        prepared.invert(&mut buf).unwrap();
        prop_assert_eq!(buf, payload, "{} {}", effect, params);
    }
}
