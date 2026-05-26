# rubrum_editor

Web-based editor UI for Rubrum charts.

Status: **placeholder** in this repository (currently a minimal Rust `main.rs`).

A functional Trunk + WASM example that renders charts via the pure-SVG backend lives at:

- `examples/trunk_wasm_svg_chart/`

## Intended architecture

- Rendering via `rubrum_svg` (pure-SVG backend)
- Shared types/spec (`Theme`, `Layout`, `ChartData`, etc.) via `rubrum_cairo` with `default-features = false`
- Must remain wasm-friendly (no Cairo / native sys deps)

## Local dev (current placeholder)

Run from the repo root:

```sh
cargo run -q --manifest-path lib/rubrum_editor/Cargo.toml
```

## Trunk/WASM (once implemented)

Prerequisites:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

Typical workflow (run from the repo root, once this crate has its own `Trunk.toml`):

```sh
trunk serve --open --config Trunk.toml
```

See `examples/trunk_wasm_svg_chart/README.md` for a known-good Trunk/WASM setup in this repo.

