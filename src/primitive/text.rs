use rubrum_render::options::RgbaColor;

/// Escape a string for safe use inside an XML attribute.
///
/// This crate keeps a small wrapper here so the pure-SVG backend can depend on a stable
/// API even if the upstream implementation moves.
pub fn escape_xml_attr(s: &str) -> String {
    rubrum_render::svg::escape_xml_attr(s)
}

/// Convert a color to `rgba(r,g,b,a)` CSS.
pub fn rgba_css(c: RgbaColor) -> String {
    rubrum_render::svg::rgba_css(c)
}

/// Convert a color to a CSS-variable expression with a theme-derived fallback.
///
/// Example output: `var(--rb-chart-text, rgba(255,255,255,1))`.
pub fn rgba_css_var(var: &str, fallback: RgbaColor) -> String {
    let fallback_css = rgba_css(fallback);
    format!("var({var}, {fallback_css})")
}

/// Convert canonical keys (e.g. `natal_bodies`) into a stable CSS token.
pub fn canonical_key_to_css_token(key: &str) -> String {
    rubrum_render::svg::canonical_key_to_css_token(key)
}
