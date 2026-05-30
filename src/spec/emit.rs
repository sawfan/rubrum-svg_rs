use rubrum_render::options::RgbaColor;

use crate::primitive::{
    hit_circle, hit_line, hit_ring, line_extra, push_svg_node, text_centered, text_centered_extra,
    use_symbol,
};
use rubrum_render::glyph_paint::GlyphPaint;

// Compatibility wrappers for the spec renderer.
//
// These preserve the indentation/whitespace patterns of earlier string emitters so downstream
// tests that do substring matching remain stable.
pub fn push_line(
    out: &mut String,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: RgbaColor,
    width: f64,
) {
    if let Some(node) = line_extra(x1, y1, x2, y2, stroke, width, None, None) {
        push_svg_node(out, "  ", node);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn push_line_extra(
    out: &mut String,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: RgbaColor,
    width: f64,
    class_attr: Option<&str>,
    extra_attrs: Option<&str>,
) {
    if let Some(node) = line_extra(x1, y1, x2, y2, stroke, width, class_attr, extra_attrs) {
        push_svg_node(out, "  ", node);
    }
}

pub fn push_hit_ring(
    out: &mut String,
    cx: f64,
    cy: f64,
    r: f64,
    hit_width: f64,
    class_attr: &str,
    extra_attrs: Option<&str>,
) {
    if let Some(node) = hit_ring(cx, cy, r, hit_width, class_attr, extra_attrs) {
        push_svg_node(out, "  ", node);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn push_hit_line(
    out: &mut String,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    width: f64,
    class_attr: &str,
    extra_attrs: Option<&str>,
) {
    if let Some(node) = hit_line(x1, y1, x2, y2, width, class_attr, extra_attrs) {
        push_svg_node(out, "      ", node);
    }
}

pub fn push_hit_circle(out: &mut String, cx: f64, cy: f64, r: f64, class_attr: &str) {
    if let Some(node) = hit_circle(cx, cy, r, class_attr) {
        push_svg_node(out, "    ", node);
    }
}

pub fn push_text(
    out: &mut String,
    x: f64,
    y: f64,
    text: &str,
    fill: RgbaColor,
    font_family: &str,
    font_size: f64,
) {
    let node = text_centered(x, y, text, fill, font_family, font_size);
    push_svg_node(out, "  ", node);
}

pub fn push_text_extra(
    out: &mut String,
    x: f64,
    y: f64,
    text: &str,
    fill: RgbaColor,
    font_family: &str,
    font_size: f64,
    class_attr: Option<&str>,
    extra_attrs: Option<&str>,
) {
    let node = text_centered_extra(
        x,
        y,
        text,
        fill,
        font_family,
        font_size,
        class_attr,
        extra_attrs,
    );
    push_svg_node(out, "  ", node);
}

pub fn glyph_paint_attrs(paint: GlyphPaint) -> String {
    let mut attrs = Vec::new();

    if let Some(color) = paint.color {
        let color_css = crate::primitive::rgba_css(color);
        attrs.push(format!(
            "color=\"{}\"",
            crate::primitive::escape_xml_attr(&color_css)
        ));
        attrs.push(format!(
            "style=\"--rb-glyph-color: {}; --rb-glyph-fill: {}; --rb-glyph-stroke: {};\"",
            crate::primitive::escape_xml_attr(&color_css),
            crate::primitive::escape_xml_attr(
                &paint
                    .fill
                    .map(crate::primitive::rgba_css)
                    .unwrap_or_else(|| color_css.clone())
            ),
            crate::primitive::escape_xml_attr(
                &paint
                    .stroke
                    .map(crate::primitive::rgba_css)
                    .unwrap_or_else(|| color_css.clone())
            )
        ));
    } else if paint.fill.is_some() || paint.stroke.is_some() {
        let fill_css = paint.fill.map(crate::primitive::rgba_css);
        let stroke_css = paint.stroke.map(crate::primitive::rgba_css);
        let mut style = String::new();
        if let Some(fill_css) = fill_css.as_ref() {
            style.push_str("--rb-glyph-fill: ");
            style.push_str(fill_css);
            style.push_str("; ");
        }
        if let Some(stroke_css) = stroke_css.as_ref() {
            style.push_str("--rb-glyph-stroke: ");
            style.push_str(stroke_css);
            style.push_str("; ");
        }
        attrs.push(format!(
            "style=\"{}\"",
            crate::primitive::escape_xml_attr(style.trim())
        ));
    }

    let fill = paint.fill.or(paint.color);
    let stroke = paint.stroke.or(paint.color);

    if let Some(fill) = fill {
        let fill_css = crate::primitive::rgba_css(fill);
        attrs.push(format!(
            "fill=\"{}\"",
            crate::primitive::escape_xml_attr(&fill_css)
        ));
    }
    if let Some(stroke) = stroke {
        let stroke_css = crate::primitive::rgba_css(stroke);
        attrs.push(format!(
            "stroke=\"{}\"",
            crate::primitive::escape_xml_attr(&stroke_css)
        ));
    }
    if let Some(opacity) = paint.fill_opacity {
        attrs.push(format!("fill-opacity=\"{}\"", opacity.clamp(0.0, 1.0)));
    }
    if let Some(opacity) = paint.stroke_opacity {
        attrs.push(format!("stroke-opacity=\"{}\"", opacity.clamp(0.0, 1.0)));
    }
    if let Some(width) = paint.stroke_width
        && width > 0.0
    {
        attrs.push(format!("stroke-width=\"{width}\""));
    }

    attrs.join(" ")
}

pub fn push_use(
    out: &mut String,
    href: &str,
    x: f64,
    y: f64,
    size: f64,
    class_attr: &str,
    extra_attrs: Option<&str>,
) {
    if let Some(node) = use_symbol(href, x, y, size, class_attr, extra_attrs) {
        push_svg_node(out, "  ", node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_paint_attrs_emits_current_color_controls() {
        let paint = GlyphPaint::monochrome(RgbaColor {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });

        let attrs = glyph_paint_attrs(paint);

        assert!(attrs.contains("color=\"rgba(255,0,0,1)\""));
        assert!(attrs.contains("fill=\"rgba(255,0,0,1)\""));
        assert!(attrs.contains("stroke=\"rgba(255,0,0,1)\""));
        assert!(attrs.contains("--rb-glyph-fill: rgba(255,0,0,1)"));
    }
}
