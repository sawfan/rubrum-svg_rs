# AGENTS

This document is for humans and automated agents working on the **rubrum_svg** crate.

The goal is to describe what the crate does, how it is organized, and how to verify changes.

---

## 1. Project summary

- **Name:** `rubrum_svg`
- **Type:** Rust library crate
- **Domain:** Astrology chart rendering
- **Purpose:** Provide a **Cairo-free** spec renderer that outputs SVG as a string.

This crate is intended to compile cleanly to `wasm32-unknown-unknown` and be used by web UIs.

Primary entrypoint:

- `chart_to_svg_string_spec(...)`

---

## 2. Dependencies

- `rubrum_render` — backend-agnostic spec/core types and render planning.
- `rubrum` — domain types (signs, bodies, aspects computation).

---

## 3. Key modules / organization

- `src/spec/` — the pure-SVG spec renderer implementation (split into focused submodules).
- `src/spec/render.rs` — top-level SVG assembly + public `chart_to_svg_string_spec` entrypoint.
- `src/primitive/` — small SVG authoring helpers (DOM element builders, text escaping, emit helpers).
- `src/lib.rs` — small facade re-exporting `chart_to_svg_string_spec`.

This crate should not depend on Cairo.

---

## 4. Verification

```sh
RUSTFLAGS='-Dwarnings' cargo check -q --manifest-path rubrum_svg_rs/Cargo.toml
RUSTFLAGS='-Dwarnings' cargo test  -q --manifest-path rubrum_svg_rs/Cargo.toml
```

WASM smoke-check (crate-level; Cairo-free):

```sh
rustup target add wasm32-unknown-unknown
RUSTFLAGS='-Dwarnings' cargo check -q --manifest-path rubrum_svg_rs/Cargo.toml \
  --target wasm32-unknown-unknown
```

## 5. Recent agent work

- Split the former monolithic `src/spec.rs` into a `src/spec/` module directory, with focused submodules:
  - `emit.rs` (SVG node emission helpers)
  - `ticks.rs` (tick metadata helpers)
  - `placement_boundary_ticks.rs` (dataset-driven tick ring rendering)
  - `placements.rs` (lane glyphs + placement label segments)
  - `houses.rs` (house spokes/axes)
  - `signs.rs` (sign dividers + labels)
  - `band.rs` (band/lane fills, boundaries, ticks, and wiring houses/signs)
  - `render.rs` (top-level SVG assembly + public `chart_to_svg_string_spec` entrypoint)

- Kept the public API stable: `rubrum_svg::chart_to_svg_string_spec(...)` remains unchanged.

- Note: when running tests, this repo relies on per-crate `.cargo/config.toml` `[patch.crates-io]` entries to point `rubrum` / `rubrum_render` at local workspace paths.

- (Older) Migrated additional SVG emission in `src/spec.rs` from string helpers (`push_text`, `push_use`) to DOM-based helpers from `rubrum_render::svg` (`text_centered`, `use_symbol`) for sign labels.
- (Older) Test runs from this repo layout require explicit `--config patch.crates-io.*` overrides (the per-crate `.cargo/config.toml` uses absolute paths that may not match all environments).

- Added dataset-aware SVG metadata for house divisions:
  - `src/spec/houses.rs` now emits `data-rb-dataset` and `data-rb-house-set` on house spokes and major axes.
- Implemented cross-dataset aspect rendering in the spec renderer when `GlyphLaneMode::CrossAspects` (or `Aspects` with `other_dataset`) is configured:
  - `src/spec/aspects.rs` computes edges via `rubrum::aspect::compute::compute_aspects_cross`.
  - Endpoints are dataset-qualified (e.g. `natal:Sun`, `transit:Sun`) to avoid collisions.
  - Endpoint radii are derived from the actual placement rings so lines span across intervening bands.


