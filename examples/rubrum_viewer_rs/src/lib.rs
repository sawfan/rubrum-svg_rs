use rubrum_render::{AspectRules, ChartData, Layout, Theme};
use wasm_bindgen::prelude::*;

#[derive(serde::Deserialize)]
struct DataWrapper {
    data: ChartData,
}

// This crate is built as a wasm-bindgen module for the browser.
//
// IMPORTANT:
// - Do not use `#[wasm_bindgen(start)]` here.
//   The HTML scaffold loads this module and expects it to be a pure library that exports
//   functions (e.g. `render_natal_svg`).
//   Auto-running DOM code at module init time can panic/trap if the expected elements are not
//   present.

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

fn parse_data(data_toml: &str) -> Result<ChartData, String> {
    let w: DataWrapper = toml::from_str(data_toml).map_err(|e| e.to_string())?;
    Ok(w.data)
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
            if sprite_url.is_empty() {
                // Use document-local fragment refs: `<use href="#rb-body-sun">`.
                out.push_str("glyph_sprite_url = \"\"\n");
            } else {
                out.push_str(&format!("glyph_sprite_url = \"{sprite_url}\"\n"));
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }

    out
}

/// Return the embedded default TOMLs used by this example.
///
/// This is used by the browser UI so it can render without requiring ephemeris computation.
#[wasm_bindgen]
pub fn embedded_natal_tomls() -> JsValue {
    // NOTE: `Trunk.toml` uses `public_url = "./"`, so we use relative asset paths.
    // Use an inline (document-local) sprite sheet so `<use href="#rb-body-sun">` works across
    // browsers (notably Firefox, which can reject external `<use>` references).
    let theme_dark_toml =
        rewrite_glyph_sprite_url(rubrum_render::embedded_configs::THEME_DARK_TOML, "");
    let theme_light_toml =
        rewrite_glyph_sprite_url(rubrum_render::embedded_configs::THEME_LIGHT_TOML, "");

    let layout_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_LAYOUT_ONLY_TOML;
    let data_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_DATA_TOML;
    let rules_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_ASPECTS_TOML;

    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"theme_dark_toml".into(), &theme_dark_toml.into());
    let _ = js_sys::Reflect::set(&obj, &"theme_light_toml".into(), &theme_light_toml.into());
    let _ = js_sys::Reflect::set(&obj, &"layout_toml".into(), &layout_toml.into());
    let _ = js_sys::Reflect::set(&obj, &"data_toml".into(), &data_toml.into());
    let _ = js_sys::Reflect::set(&obj, &"rules_toml".into(), &rules_toml.into());
    obj.into()
}

/// Render a natal wheel chart SVG.
///
/// Inputs are TOML strings so the UI can support theme overrides without rebuilding.
#[wasm_bindgen]
pub fn render_natal_svg(
    theme_toml: &str,
    layout_toml: &str,
    data_toml: &str,
    rules_toml: &str,
) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();

    let theme = parse_theme(theme_toml).map_err(|e| js_err(format!("theme parse error: {e}")))?;
    let layout =
        parse_layout(layout_toml).map_err(|e| js_err(format!("layout parse error: {e}")))?;
    let data = parse_data(data_toml).map_err(|e| js_err(format!("data parse error: {e}")))?;
    let rules = parse_rules(rules_toml).map_err(|e| js_err(format!("rules parse error: {e}")))?;

    rubrum_svg::chart_to_svg_string_spec(&theme, &layout, Some(&rules), &data)
        .map_err(|e| js_err(format!("render error: {e}")))
}

/// Render an aspect grid SVG for a natal dataset.
#[wasm_bindgen]
pub fn render_aspect_grid_svg(
    theme_toml: &str,
    data_toml: &str,
    rules_toml: &str,
) -> Result<String, JsValue> {
    console_error_panic_hook::set_once();

    let theme = parse_theme(theme_toml).map_err(|e| js_err(format!("theme parse error: {e}")))?;
    let data = parse_data(data_toml).map_err(|e| js_err(format!("data parse error: {e}")))?;
    let rules = parse_rules(rules_toml).map_err(|e| js_err(format!("rules parse error: {e}")))?;

    let opts = rubrum_svg::AspectGridSvgOptions {
        dataset_id: "natal",
        house_set_id: "natal",
        ..Default::default()
    };

    rubrum_svg::aspect_grid_to_svg_string(&theme, Some(&rules), &data, opts)
        .map_err(|e| js_err(format!("render error: {e}")))
}
