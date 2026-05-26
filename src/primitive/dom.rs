use svg::node::element::{Circle, Line, Path, Text, Use};

use rubrum_render::options::RgbaColor;

use super::{escape_xml_attr, rgba_css_var};

/// Parse a string like `key="value" key2="value2"` into key-value pairs.
///
/// This exists to support legacy call sites that want to pass through extra attributes.
///
/// IMPORTANT: Attribute values may contain spaces (e.g. `fill="var(--x, rgba(1, 2, 3, 0.4))"`).
/// We must therefore parse quoted values rather than splitting on whitespace.
pub fn parse_extra_attrs(extra_attrs: &str) -> Vec<(Box<str>, Box<str>)> {
    let mut out: Vec<(Box<str>, Box<str>)> = Vec::new();

    let bytes = extra_attrs.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        // Skip leading whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        // Parse key up to '=' or whitespace.
        let key_start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' {
            i += 1;
        }
        let key = extra_attrs[key_start..i].trim();
        if key.is_empty() {
            break;
        }

        // Skip optional whitespace before '='.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        // Require '='.
        if i >= bytes.len() || bytes[i] != b'=' {
            // Malformed token; skip to next whitespace.
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        i += 1;

        // Skip whitespace after '='.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        // Parse value: quoted (preferred) or bare until whitespace.
        let quote = bytes[i];
        let value: &str;
        if quote == b'"' || quote == b'\'' {
            i += 1;
            let value_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            value = &extra_attrs[value_start..i];
            if i < bytes.len() {
                i += 1; // consume closing quote
            }
        } else {
            let value_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            value = &extra_attrs[value_start..i];
        }

        out.push((
            key.to_owned().into_boxed_str(),
            value.to_owned().into_boxed_str(),
        ));
    }

    out
}

pub fn apply_extra_attrs<T>(mut node: T, extra_attrs: Option<&str>) -> T
where
    T: svg::Node,
{
    if let Some(extra) = extra_attrs {
        for (k, v) in parse_extra_attrs(extra) {
            node.assign(k.as_ref(), v.as_ref());
        }
    }

    node
}

pub fn set_class_attr<T>(mut node: T, class_attr: Option<&str>) -> T
where
    T: svg::Node,
{
    if let Some(class_attr) = class_attr {
        node.assign("class", class_attr);
    }
    node
}

pub fn circle_extra(
    cx: f64,
    cy: f64,
    r: f64,
    stroke: RgbaColor,
    width: f64,
    class_attr: Option<&str>,
    extra_attrs: Option<&str>,
) -> Option<Circle> {
    if r <= 0.0 || width <= 0.0 {
        return None;
    }

    let stroke_attr = rgba_css_var("--rb-chart-structure", stroke);

    let circle = Circle::new()
        .set("cx", cx)
        .set("cy", cy)
        .set("r", r)
        .set("fill", "none")
        .set("stroke", stroke_attr)
        .set("stroke-width", width);

    let circle = set_class_attr(circle, class_attr);
    Some(apply_extra_attrs(circle, extra_attrs))
}

#[allow(clippy::too_many_arguments)]
pub fn line_extra(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    stroke: RgbaColor,
    width: f64,
    class_attr: Option<&str>,
    extra_attrs: Option<&str>,
) -> Option<Line> {
    if width <= 0.0 {
        return None;
    }

    let stroke_attr = rgba_css_var("--rb-chart-structure", stroke);

    let line = Line::new()
        .set("x1", x1)
        .set("y1", y1)
        .set("x2", x2)
        .set("y2", y2)
        .set("stroke", stroke_attr)
        .set("stroke-width", width);

    let line = set_class_attr(line, class_attr);
    Some(apply_extra_attrs(line, extra_attrs))
}

/// Build an even-odd annulus path (ring) around `(cx, cy)`.
///
/// - Outer arc is clockwise.
/// - Inner arc is counter-clockwise.
///
/// This matches the legacy implementation in `rubrum_render_rs` and is used for band/lane fills.
pub fn annulus_path(cx: f64, cy: f64, r_inner: f64, r_outer: f64) -> String {
    if r_outer <= 0.0 {
        return String::new();
    }

    if r_inner <= 0.0 {
        // Just a circle.
        return format!(
            "M {} {} A {} {} 0 1 0 {} {} A {} {} 0 1 0 {} {} Z",
            cx + r_outer,
            cy,
            r_outer,
            r_outer,
            cx - r_outer,
            cy,
            r_outer,
            r_outer,
            cx + r_outer,
            cy
        );
    }

    format!(
        "M {} {} A {} {} 0 1 0 {} {} A {} {} 0 1 0 {} {} Z \
M {} {} A {} {} 0 1 1 {} {} A {} {} 0 1 1 {} {} Z",
        cx + r_outer,
        cy,
        r_outer,
        r_outer,
        cx - r_outer,
        cy,
        r_outer,
        r_outer,
        cx + r_outer,
        cy,
        cx + r_inner,
        cy,
        r_inner,
        r_inner,
        cx - r_inner,
        cy,
        r_inner,
        r_inner,
        cx + r_inner,
        cy
    )
}

pub fn hit_ring(
    cx: f64,
    cy: f64,
    r: f64,
    hit_width: f64,
    class_attr: &str,
    extra_attrs: Option<&str>,
) -> Option<Circle> {
    if r <= 0.0 || hit_width <= 0.0 {
        return None;
    }

    let circle = Circle::new()
        .set("cx", cx)
        .set("cy", cy)
        .set("r", r)
        .set("fill", "none")
        .set("stroke", "transparent")
        .set("stroke-width", hit_width)
        .set("stroke-linecap", "round")
        .set("pointer-events", "stroke")
        .set("class", class_attr);

    Some(apply_extra_attrs(circle, extra_attrs))
}

pub fn hit_line(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    width: f64,
    class_attr: &str,
    extra_attrs: Option<&str>,
) -> Option<Line> {
    if width <= 0.0 {
        return None;
    }

    let line = Line::new()
        .set("x1", x1)
        .set("y1", y1)
        .set("x2", x2)
        .set("y2", y2)
        .set("fill", "none")
        .set("stroke", "transparent")
        .set("stroke-width", width)
        .set("stroke-linecap", "round")
        .set("pointer-events", "stroke")
        .set("class", class_attr);

    Some(apply_extra_attrs(line, extra_attrs))
}

pub fn hit_circle(cx: f64, cy: f64, r: f64, class_attr: &str) -> Option<Circle> {
    if r <= 0.0 {
        return None;
    }

    // Transparent fill counts as painted but stays invisible.
    Some(
        Circle::new()
            .set("cx", cx)
            .set("cy", cy)
            .set("r", r)
            .set("fill", "transparent")
            .set("pointer-events", "all")
            .set("class", class_attr),
    )
}

pub fn text_centered(
    x: f64,
    y: f64,
    text: &str,
    fill: RgbaColor,
    font_family: &str,
    font_size: f64,
) -> Text {
    let fill_attr = rgba_css_var("--rb-chart-text", fill);

    // svg crate escapes text nodes, but we also ensure minimal XML safety in case this is later
    // used as an attribute.
    let _ = escape_xml_attr(text);

    Text::new(text)
        .set("x", x)
        .set("y", y)
        .set("text-anchor", "middle")
        .set("dominant-baseline", "central")
        .set("fill", fill_attr)
        .set("font-family", font_family)
        .set("font-size", font_size)
}

pub fn text_centered_extra(
    x: f64,
    y: f64,
    text: &str,
    fill: RgbaColor,
    font_family: &str,
    font_size: f64,
    class_attr: Option<&str>,
    extra_attrs: Option<&str>,
) -> Text {
    let mut node = text_centered(x, y, text, fill, font_family, font_size);

    if let Some(class_attr) = class_attr {
        node = node.set("class", class_attr);
    }

    apply_extra_attrs(node, extra_attrs)
}

pub fn use_symbol(
    href: &str,
    x: f64,
    y: f64,
    size: f64,
    class_attr: &str,
    extra_attrs: Option<&str>,
) -> Option<Use> {
    if size <= 0.0 {
        return None;
    }

    // Center the symbol on (x, y).
    //
    // NOTE: Prefer explicit `x`/`y` positioning over a `transform: translate(...)`.
    // Some browsers have quirks when scaling external `<symbol>` references via `<use>`
    // if positioning is done only through transforms.
    let x0 = x - (size / 2.0);
    let y0 = y - (size / 2.0);

    // `xlink:href` is deprecated but still useful for older SVG implementations.
    //
    // Important: only set `xlink:href` when this is an external reference.
    // Some browsers (notably Firefox) can throw when `xlink:href` is set to a fragment-only
    // reference like `#rb-sign-aries`.
    let mut use_elem = Use::new()
        .set("class", class_attr)
        .set("href", href)
        .set("x", x0)
        .set("y", y0)
        .set("width", size)
        .set("height", size)
        .set("preserveAspectRatio", "xMidYMid meet")
        .set("overflow", "visible");

    if !href.starts_with('#') {
        use_elem = use_elem.set("xlink:href", href);
    }

    Some(apply_extra_attrs(use_elem, extra_attrs))
}

pub fn path_d(d: &str, class_attr: Option<&str>, extra_attrs: Option<&str>) -> Option<Path> {
    let d = d.trim();
    if d.is_empty() {
        return None;
    }

    let path = Path::new().set("d", d);

    let path = set_class_attr(path, class_attr);
    let path = apply_extra_attrs(path, extra_attrs);

    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kv_pairs() {
        let attrs = parse_extra_attrs("data-a=\"1\" data-b=\"two\"");
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].0.as_ref(), "data-a");
        assert_eq!(attrs[0].1.as_ref(), "1");
    }
}
