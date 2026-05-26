use rubrum_render::{AspectRules, ChartData, Layout, Theme};
use wasm_bindgen::prelude::*;

// This crate is built as a wasm-bindgen module for the browser.
//
// IMPORTANT:
// - Do not use `#[wasm_bindgen(start)]` here.
//   The web scaffold loads this module and expects it to be a pure library
//   that exports functions (e.g. `render_chart_svg`).
//   Auto-running DOM code at module init time will panic/trap if the expected
//   elements are not present.

#[derive(serde::Deserialize)]
struct ThemeWrapper {
    theme: Theme,
}

#[derive(serde::Deserialize)]
struct LayoutWrapper {
    layout: Layout,
}

#[derive(serde::Deserialize)]
struct RulesWrapper {
    rules: AspectRules,
}

fn parse_theme(theme_toml: &str) -> Result<Theme, String> {
    let w: ThemeWrapper = toml::from_str(theme_toml).map_err(|e| e.to_string())?;
    Ok(w.theme)
}

fn parse_layout(layout_toml: &str) -> Result<Layout, String> {
    let w: LayoutWrapper = toml::from_str(layout_toml).map_err(|e| e.to_string())?;
    Ok(w.layout)
}

fn parse_rules(rules_toml: &str) -> Result<AspectRules, String> {
    let w: RulesWrapper = toml::from_str(rules_toml).map_err(|e| e.to_string())?;
    Ok(w.rules)
}

fn js_err(msg: impl AsRef<str>) -> JsValue {
    JsValue::from_str(msg.as_ref())
}

fn rewrite_glyph_sprite_url(theme_toml: &str, sprite_url: &str) -> String {
    // The theme TOML includes a `glyph_sprite_url` setting used by the SVG renderer.
    // Trunk serves files from `public/` at the site root (respecting `public_url`).
    // We rewrite the setting here so glyphs are loaded from the correct place without relying on
    // hard-coded paths in the upstream embedded theme.
    let mut out = String::new();

    for line in theme_toml.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("glyph_sprite_url") {
            out.push_str(&format!("glyph_sprite_url = \"{}\"\n", sprite_url));
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

/// Render a Rubrum `Chart` JSON string to an SVG string.
///
/// The input JSON is expected to be the serialized output of `rubrum::Chart`.
///
/// This is intentionally a small API surface:
/// - JS computes a chart (via the WASI SwissEph module)
/// - JS passes the resulting chart JSON into this function
/// - Rust returns the SVG markup as a string
#[wasm_bindgen]
pub fn render_chart_svg(chart_json: &str) -> Result<String, JsValue> {
    // Convert Rust panics into readable console errors instead of `unreachable executed`.
    console_error_panic_hook::set_once();

    let chart: rubrum::Chart =
        serde_json::from_str(chart_json).map_err(|e| js_err(format!("invalid chart JSON: {e}")))?;

    let data = ChartData::from(&chart);

    // For now we use embedded defaults. This keeps the browser build independent of file access.
    //
    // NOTE: Trunk copies `web/assets/glyphs_white.svg` to `dist/assets/glyphs_white.svg`, so the
    // correct absolute URL at runtime (with `public_url = "/"`) is:
    //   /assets/glyphs_white.svg
    //
    // The upstream embedded theme currently uses `/public/glyphs_white.svg`, so we rewrite it.
    let theme_toml = rewrite_glyph_sprite_url(
        rubrum_render::embedded_configs::THEME_DARK_TOML,
        "/assets/glyphs_white.svg",
    );
    let layout_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_LAYOUT_ONLY_TOML;
    let rules_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_ASPECTS_TOML;

    let theme = parse_theme(&theme_toml).map_err(|e| js_err(format!("theme parse error: {e}")))?;
    let layout =
        parse_layout(layout_toml).map_err(|e| js_err(format!("layout parse error: {e}")))?;
    let rules = parse_rules(rules_toml).map_err(|e| js_err(format!("rules parse error: {e}")))?;

    rubrum_svg::chart_to_svg_string_spec(&theme, &layout, Some(&rules), &data)
        .map_err(|e| js_err(format!("render error: {e}")))
}
