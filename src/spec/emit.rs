use rubrum_render::options::RgbaColor;

use crate::primitive::{
    hit_circle, hit_line, hit_ring, line_extra, push_svg_node, text_centered, text_centered_extra,
    use_symbol,
};

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
