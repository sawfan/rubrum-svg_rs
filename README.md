# rubrum_svg

Pure-SVG rendering backend for Rubrum charts.

This crate provides a Cairo-free spec renderer that outputs an SVG string. It is intended to compile cleanly to `wasm32-unknown-unknown`.

## API

Primary entrypoint:

- `rubrum_svg::chart_to_svg_string_spec(...)`

## Build / test

```sh
RUSTFLAGS='-Dwarnings' cargo check -q --manifest-path lib/rubrum_svg/Cargo.toml
RUSTFLAGS='-Dwarnings' cargo test  -q --manifest-path lib/rubrum_svg/Cargo.toml
```

## WASM smoke-check

```sh
rustup target add wasm32-unknown-unknown
RUSTFLAGS='-Dwarnings' cargo check -q --manifest-path lib/rubrum_svg/Cargo.toml --target wasm32-unknown-unknown
```

## Related

- Native Cairo renderer: `rubrum_cairo`
- Shared spec/types: `rubrum_render`

