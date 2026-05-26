# AGENTS

This document is for humans and automated agents working on the **rubrum_viewer** crate.

---

## 1. Project summary

- **Name:** `rubrum_viewer`
- **Type:** Rust binary crate (currently placeholder)
- **Intended domain:** Web-based chart viewer UI
- **Intended purpose:** Provide a lightweight viewer for rendering Rubrum charts via a browser-friendly SVG backend.

Repository note:

- In this repo, `rubrum_viewer` currently contains only a minimal `main.rs` placeholder.
- For a functional WASM/Trunk example, see `examples/trunk_wasm_svg_chart/`.

---

## 2. Intended architecture (when implemented)

- Render via `rubrum_svg`.
- Use `rubrum_cairo` with `default-features = false` to access shared types.
- Keep dependencies wasm-compatible.

---

## 3. Verification

Current placeholder verification:

```sh
RUSTFLAGS='-Dwarnings' cargo check -q --manifest-path lib/rubrum_viewer/Cargo.toml
cargo run -q --manifest-path lib/rubrum_viewer/Cargo.toml
```

If/when converted into a Trunk/WASM app, verify a wasm build:

```sh
rustup target add wasm32-unknown-unknown
trunk build
```

