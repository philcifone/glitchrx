# CLAUDE.md

Operational rules for agents working on glitcharaxia. `SPEC.md` is the source of truth for behaviour, and `VOCAB.md` defines the terms.

## Gate

Run `just check` (or `scripts/check.sh` if `just` is not installed) before calling any work done. It runs:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

Report the real output. Never assume success.

## Git

- `main` holds the bootstrap only. Work happens on the phase branch (`phase-1-cli`, ...) and on feature branches cut from it.
- Merge feature branches back into the phase branch once the gate is green.
- Never push. Never merge into `main`; Phil does that after review.
- Commit small and often. Use Conventional Commits (`feat:`, `fix:`, `test:`, `docs:`, `chore:`, `refactor:`). No AI attribution lines.
- At each phase boundary, stop and summarize: what was built, test results, deviations from the spec, open questions, next step, and the effort-level outlook. Do not start the next phase without Phil's go-ahead.

## Invariants (tests enforce these)

- **Header-safe.** In safe mode, every byte outside the entropy-coded data is identical to the input.
- **Length-preserving.** Byte effects never change the payload length.
- **Inputs are never written,** even with `--force`. Outputs are never overwritten without `--force`.
- **Determinism:**
  - integer math only (gains quantized to Q8)
  - our own sampling helpers on ChaCha8
  - a per-effect `VERSION`, bumped whenever an algorithm's output changes
- **No network calls** at runtime.

## Style

- No emojis in any file, including markdown.
- Well-commented, conventional Rust. Add a brief note where a technique is interesting.
- Keep `VOCAB.md` updated when a new term enters the conversation or the code.
- State assumptions and proceed. Ask only when the answer materially changes the work.
- After a tangent, give a short "current status / next step" recap.
- Dev servers bind to 0.0.0.0 only when asked or for development. Report the port and LAN URL.
