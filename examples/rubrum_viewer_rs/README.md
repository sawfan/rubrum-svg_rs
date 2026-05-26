# rubrum_viewer

Web-based viewer UI for Rubrum charts.

Status: Trunk + WASM example.

This example renders:
- a natal wheel chart SVG
- a natal aspect-grid SVG

via the pure-SVG backend (`rubrum_svg`) using embedded TOML defaults from `rubrum_render`.

## Intended architecture

- Rendering via `rubrum_svg` (pure-SVG backend)
- Shared types/spec (`Theme`, `Layout`, `ChartData`, etc.) via `rubrum_cairo` with `default-features = false`
- Keep dependencies wasm-compatible

## Local dev (current placeholder)

Run from the repo root:

```sh
cargo run -q --manifest-path lib/rubrum_viewer/Cargo.toml
```

## Trunk/WASM (once implemented)

Prerequisites:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

Typical workflow (run from the repo root, once this crate has its own `Trunk.toml`):

```sh
trunk serve --open --config lib/rubrum_viewer/Trunk.toml
```

See `examples/trunk_wasm_svg_chart/README.md` for a known-good Trunk/WASM setup in this repo.

