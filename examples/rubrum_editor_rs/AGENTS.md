# AGENTS

This document is for humans and automated agents working on the **rubrum_editor** crate.

---

## 1. Project summary

- **Name:** `rubrum_editor`
- **Type:** Rust binary crate (currently placeholder)
- **Intended domain:** Web-based chart editor UI
- **Intended purpose:** Provide an interactive editor for Rubrum charts (theme/layout/data iteration, selection tools, etc.).

Repository note:

- In this repo, `rubrum_editor` currently contains only a minimal `main.rs` placeholder.
- The active WASM/Trunk proof-of-concept lives at `examples/trunk_wasm_svg_chart/`.

---

## 2. Intended architecture (when implemented)

- Render charts via `rubrum_svg` (pure-SVG backend).
- Depend on `rubrum_cairo` with `default-features = false` for shared types/spec (`Theme`, `Layout`, `ChartData`, etc.).
- Keep the crate wasm-friendly (avoid Cairo / native sys deps).

---

## 3. Verification

Current placeholder verification:

```sh
RUSTFLAGS='-Dwarnings' cargo check -q --manifest-path lib/rubrum_editor/Cargo.toml
cargo run -q --manifest-path lib/rubrum_editor/Cargo.toml
```

If/when converted into a Trunk/WASM app, also verify wasm builds:

```sh
rustup target add wasm32-unknown-unknown
trunk build
```

