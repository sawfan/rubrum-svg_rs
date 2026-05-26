# rubrum_viewer_wasm

`wasm-bindgen` export crate for rendering Rubrum charts to SVG in the browser.

This crate is intended to be the **thin wasm boundary**:

- Input: `rubrum::Chart` JSON (string)
- Output: SVG markup (string)

The browser-facing JS in this repository loads this crate's wasm-bindgen bundle from `web/pkg/`.

## Build (wasm-pack)

From the repo root:

```sh
wasm-pack build --release --target web --out-name rubrum_viewer_web --out-dir ./web/pkg ./librubrum/rubrum_viewer_wasm
```

Or use the repository helper:

```sh
./scripts/build_wasm.sh
```
