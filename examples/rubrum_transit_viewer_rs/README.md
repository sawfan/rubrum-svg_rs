# rubrum_transit_viewer

Trunk + WASM example for rendering a **transit chart** with Rubrum's pure-SVG backend.

This example is specifically set up to demonstrate a chart with **two datasets**:

- `natal`
- `transit`

The UI shows:

- the transit layout TOML
- the full data TOML
- extracted dataset previews for `natal` and `transit`
- the rendered SVG chart

## Run

From the repo root:

```sh
trunk serve --open --config rubrum_svg_rs/examples/rubrum_transit_viewer_rs/Trunk.toml
```

## Notes

- Default TOMLs come from `rubrum_render::embedded_configs`.
- Glyph sprite is served from `public/glyphs_white.svg`.

