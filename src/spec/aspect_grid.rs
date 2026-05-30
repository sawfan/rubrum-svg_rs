use std::collections::BTreeMap;

use rubrum::aspect::compute_aspects_natal;
use rubrum::{AspectEndpointId, AspectRules, DegreeAspectKind, EndpointKey, House, Occupant};
use rubrum_render::aspects::resolve_aspect_stroke_style;
use rubrum_render::chart_data::{ChartData, HouseCuspData};
use rubrum_render::error::ChartRenderError;
use rubrum_render::glyph_paint::{
    GlyphPaint, resolve_occupant_glyph_paint, resolve_sign_glyph_paint, sign_element,
};
use rubrum_render::glyphs::{
    angle_svg_symbol_id, body_svg_symbol_id, chart_point_svg_symbol_id, occupant_label,
    sign_svg_symbol_id,
};
use rubrum_render::options::RgbaColor;
use rubrum_render::theme::Theme;

use svg::Document;
use svg::node::Text as TextNode;
use svg::node::element::{Group, Rectangle, Text, Use};

use crate::primitive::{
    canonical_key_to_css_token as key_to_css_token, rgba_css, rgba_css_var as rgba_css_var_prim,
};

fn aspect_grid_canvas_bg(theme: &Theme) -> RgbaColor {
    if let Some(palette) = theme.svg.aspect_grid {
        if let Some(bg) = palette.canvas_bg {
            return bg;
        }
    }

    // Default: match the chart background.
    theme.effective_cairo_background()
}

fn aspect_grid_cell_bg(theme: &Theme) -> RgbaColor {
    if let Some(palette) = theme.svg.aspect_grid {
        if let Some(bg) = palette.cell_bg {
            return bg;
        }
    }

    // Default: a subtle surface distinct from the canvas.
    let base = theme.effective_base_colors();
    match theme.color_mode.as_ref().map(|m| m.mode) {
        Some(rubrum_render::theme::ColorMode::Dark) => RgbaColor {
            r: (base.background.r + 0.04).min(1.0),
            g: (base.background.g + 0.04).min(1.0),
            b: (base.background.b + 0.04).min(1.0),
            a: base.background.a,
        },
        _ => RgbaColor {
            r: (base.background.r - 0.04).max(0.0),
            g: (base.background.g - 0.04).max(0.0),
            b: (base.background.b - 0.04).max(0.0),
            a: base.background.a,
        },
    }
}

fn aspect_grid_grid_line(theme: &Theme) -> RgbaColor {
    if let Some(palette) = theme.svg.aspect_grid {
        if let Some(c) = palette.grid_line {
            return c;
        }
    }

    theme.effective_structure_color()
}

fn aspect_grid_text(theme: &Theme) -> RgbaColor {
    if let Some(palette) = theme.svg.aspect_grid {
        if let Some(c) = palette.text {
            return c;
        }
    }

    theme.effective_text_color()
}

fn rgba_css_var(var: &str, fallback: RgbaColor) -> String {
    let fallback_css = rgba_css(fallback);
    format!("var({var}, {fallback_css})")
}

/// Options for rendering an aspect grid ("aspect table") as a standalone SVG.
#[derive(Debug, Clone)]
pub struct AspectGridSvgOptions<'a> {
    /// Dataset id to read placements from.
    pub dataset_id: &'a str,

    /// House cusp set id used to derive house numbers.
    pub house_set_id: &'a str,

    /// SVG outer margin.
    pub margin_px: f64,

    /// Square size for matrix cells.
    pub cell_px: f64,

    /// Height of each row in the left placement list.
    ///
    /// If `None`, this defaults to `cell_px`.
    pub row_height_px: Option<f64>,

    /// Font size for label text.
    pub font_size_px: f64,

    /// Font size for glyphs rendered as text fallbacks.
    pub glyph_font_size_px: f64,

    /// If true, render glyph labels along the diagonal/right-edge of the staircase.
    ///
    /// Note: we intentionally do *not* render a redundant bottom axis row.
    pub axis_labels: bool,
}

impl Default for AspectGridSvgOptions<'_> {
    fn default() -> Self {
        Self {
            dataset_id: "natal",
            house_set_id: "natal",
            margin_px: 16.0,
            cell_px: 22.0,
            row_height_px: None,
            font_size_px: 12.0,
            glyph_font_size_px: 14.0,
            axis_labels: true,
        }
    }
}

/// Composable aspect-grid output for downstream apps.
#[derive(Debug, Clone)]
pub struct AspectGridSvgGroup {
    pub width: f64,
    pub height: f64,
    pub group: Group,
}

#[derive(Debug, Clone)]
struct GridRow {
    occupant: Occupant,
    endpoint_id: AspectEndpointId,
    sign: rubrum::Sign,
    deg: i32,
    min: i32,
    house_num: Option<i32>,
}

fn house_to_index(h: House) -> usize {
    match h {
        House::First => 0,
        House::Second => 1,
        House::Third => 2,
        House::Fourth => 3,
        House::Fifth => 4,
        House::Sixth => 5,
        House::Seventh => 6,
        House::Eighth => 7,
        House::Ninth => 8,
        House::Tenth => 9,
        House::Eleventh => 10,
        House::Twelfth => 11,
    }
}

fn derive_house_number(cusps: &[HouseCuspData], lon_deg360: f64) -> Option<i32> {
    if cusps.len() < 12 {
        return None;
    }

    // Expect one cusp per house; take the latest entry for each house if duplicates exist.
    let mut cusp_degs: [Option<f64>; 12] = [None; 12];
    for c in cusps {
        let idx = house_to_index(c.house);
        cusp_degs[idx] = Some(c.sign_degree.degrees);
    }

    if cusp_degs.iter().any(|v| v.is_none()) {
        return None;
    }

    let cusp_degs: [f64; 12] = cusp_degs.map(|v| v.unwrap_or(0.0));

    for i in 0..12 {
        let start = cusp_degs[i];
        let end = cusp_degs[(i + 1) % 12];

        let in_interval = if start <= end {
            lon_deg360 >= start && lon_deg360 < end
        } else {
            // Wrap across 360 → 0.
            lon_deg360 >= start || lon_deg360 < end
        };

        if in_interval {
            return Some((i as i32) + 1);
        }
    }

    None
}

fn occupant_display_name(occupant: Occupant) -> String {
    match occupant {
        Occupant::Empty => "".to_owned(),
        Occupant::Body(body) => body.to_string(),
        Occupant::Angle(angle) => angle.to_string(),
        Occupant::ChartPoint(point) => point.to_string(),
        Occupant::Lot(lot) => lot.to_string(),
    }
}

fn occupant_symbol_href(theme: &Theme, occupant: Occupant) -> Option<String> {
    let sprite = theme.svg.glyph_sprite_url.as_deref()?;

    let symbol_id = match occupant {
        Occupant::Body(b) => Some(body_svg_symbol_id(b)),
        Occupant::Angle(a) => Some(angle_svg_symbol_id(a)),
        Occupant::ChartPoint(p) => Some(chart_point_svg_symbol_id(p)),
        // Lots are not currently present in our sprite conventions.
        Occupant::Lot(_) | Occupant::Empty => None,
    }?;

    Some(format!("{sprite}#{symbol_id}"))
}

fn sign_symbol_href(theme: &Theme, sign: rubrum::Sign) -> Option<String> {
    let sprite = theme.svg.glyph_sprite_url.as_deref()?;
    let symbol_id = sign_svg_symbol_id(sign);
    Some(format!("{sprite}#{symbol_id}"))
}

fn aspect_kind_color(theme: &Theme, kind: &DegreeAspectKind) -> RgbaColor {
    resolve_aspect_stroke_style(
        &theme.aspects,
        kind,
        match kind {
            DegreeAspectKind::Trine | DegreeAspectKind::Sextile => RgbaColor {
                r: 0.16,
                g: 0.36,
                b: 0.85,
                a: 1.0,
            },
            DegreeAspectKind::Square | DegreeAspectKind::Opposition => RgbaColor {
                r: 0.85,
                g: 0.18,
                b: 0.18,
                a: 1.0,
            },
            _ => theme.effective_text_color(),
        },
        theme.cairo.stroke_width,
    )
    .color
}

fn occupant_sort_key(occ: Occupant) -> (i32, i32) {
    // Stable ordering that matches typical aspect tables:
    // bodies first (Sun..Pluto..Chiron), then points/lots, then angles.
    let group = match occ {
        Occupant::Body(_) => 0,
        Occupant::ChartPoint(_) | Occupant::Lot(_) => 1,
        Occupant::Angle(_) => 2,
        Occupant::Empty => 3,
    };

    let within = match occ {
        Occupant::Body(b) => match b {
            rubrum::Body::Sun => 0,
            rubrum::Body::Moon => 1,
            rubrum::Body::Mercury => 2,
            rubrum::Body::Venus => 3,
            rubrum::Body::Mars => 4,
            rubrum::Body::Jupiter => 5,
            rubrum::Body::Saturn => 6,
            rubrum::Body::Uranus => 7,
            rubrum::Body::Neptune => 8,
            rubrum::Body::Pluto => 9,
            rubrum::Body::Chiron => 10,
            _ => 99,
        },
        Occupant::ChartPoint(p) => match p {
            rubrum::ChartPoint::TrueNode => 0,
            rubrum::ChartPoint::MeanApog => 1,
            _ => 99,
        },
        Occupant::Lot(l) => match l {
            rubrum::Lot::Fortune => 10,
            _ => 99,
        },
        Occupant::Angle(a) => match a {
            rubrum::Angle::Ascendant => 0,
            rubrum::Angle::Midheaven => 1,
            _ => 99,
        },
        Occupant::Empty => 999,
    };

    (group, within)
}

fn cell_id(a: &AspectEndpointId, b: &AspectEndpointId) -> (String, String) {
    // Treat aspects as undirected for the table.
    if a.0 <= b.0 {
        (a.0.clone(), b.0.clone())
    } else {
        (b.0.clone(), a.0.clone())
    }
}

fn rect_node(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: &str,
    stroke: &str,
    stroke_width: f64,
) -> Rectangle {
    Rectangle::new()
        .set("x", x)
        .set("y", y)
        .set("width", w)
        .set("height", h)
        .set("fill", fill)
        .set("stroke", stroke)
        .set("stroke-width", stroke_width)
}

fn centered_text_node(
    x: f64,
    y: f64,
    text: &str,
    fill: &str,
    font_family: &str,
    font_size: f64,
    class_attr: Option<&str>,
) -> Text {
    let mut node = Text::new("")
        .set("x", x)
        .set("y", y)
        .set("text-anchor", "middle")
        .set("dominant-baseline", "central")
        .set("fill", fill)
        .set("font-family", font_family)
        .set("font-size", font_size)
        .add(TextNode::new(text));

    if let Some(class_attr) = class_attr {
        node = node.set("class", class_attr);
    }

    node
}

fn left_text_node(
    x: f64,
    y: f64,
    text: &str,
    fill: &str,
    font_family: &str,
    font_size: f64,
    class_attr: Option<&str>,
) -> Text {
    let mut node = Text::new("")
        .set("x", x)
        .set("y", y)
        .set("text-anchor", "start")
        .set("dominant-baseline", "central")
        .set("fill", fill)
        .set("font-family", font_family)
        .set("font-size", font_size)
        .add(TextNode::new(text));

    if let Some(class_attr) = class_attr {
        node = node.set("class", class_attr);
    }

    node
}

fn glyph_color_css(paint: GlyphPaint, fallback: RgbaColor) -> String {
    rgba_css(
        paint
            .color
            .or(paint.fill)
            .or(paint.stroke)
            .unwrap_or(fallback),
    )
}

fn apply_glyph_paint_attrs(mut node: Use, paint: GlyphPaint) -> Use {
    if let Some(color) = paint.color {
        let color_css = rgba_css(color);
        node = node.set("color", color_css.clone()).set(
            "style",
            format!(
                "--rb-glyph-color: {}; --rb-glyph-fill: {}; --rb-glyph-stroke: {};",
                color_css,
                paint
                    .fill
                    .map(rgba_css)
                    .unwrap_or_else(|| color_css.clone()),
                paint
                    .stroke
                    .map(rgba_css)
                    .unwrap_or_else(|| color_css.clone())
            ),
        );
    } else if paint.fill.is_some() || paint.stroke.is_some() {
        let mut parts = Vec::new();
        if let Some(fill) = paint.fill {
            parts.push(format!("--rb-glyph-fill: {};", rgba_css(fill)));
        }
        if let Some(stroke) = paint.stroke {
            parts.push(format!("--rb-glyph-stroke: {};", rgba_css(stroke)));
        }
        node = node.set("style", parts.join(" "));
    }

    if let Some(fill) = paint.fill.or(paint.color) {
        node = node.set("fill", rgba_css(fill));
    }
    if let Some(stroke) = paint.stroke.or(paint.color) {
        node = node.set("stroke", rgba_css(stroke));
    }
    if let Some(opacity) = paint.fill_opacity {
        node = node.set("fill-opacity", opacity.clamp(0.0, 1.0));
    }
    if let Some(opacity) = paint.stroke_opacity {
        node = node.set("stroke-opacity", opacity.clamp(0.0, 1.0));
    }
    if let Some(width) = paint.stroke_width
        && width > 0.0
    {
        node = node.set("stroke-width", width);
    }

    node
}

fn use_node(
    href: &str,
    x: f64,
    y: f64,
    size: f64,
    class_attr: &str,
    paint: Option<GlyphPaint>,
) -> Use {
    // Use `transform=translate(...)` for compatibility with earlier emitters.
    let node = Use::new()
        .set("href", href)
        .set("xlink:href", href)
        .set("transform", format!("translate({x} {y})"))
        .set("width", size)
        .set("height", size)
        .set("preserveAspectRatio", "xMidYMid meet")
        .set("overflow", "visible")
        .set("class", class_attr);

    if let Some(paint) = paint {
        apply_glyph_paint_attrs(node, paint)
    } else {
        node
    }
}

/// Render an aspect grid ("aspect table") for a single dataset as a `svg::Document`.
///
/// This is a *non-wheel* render target intended for diagnostic tables / exports.
pub fn aspect_grid_to_svg_document(
    theme: &Theme,
    aspect_rules: Option<&AspectRules>,
    data: &ChartData,
    opts: AspectGridSvgOptions<'_>,
) -> Result<Document, ChartRenderError> {
    let grid = aspect_grid_to_svg_group(theme, aspect_rules, data, opts.clone())?;

    let bg = aspect_grid_canvas_bg(theme);
    let bg_css = rgba_css_var("--rb-aspect-grid-canvas-bg", bg);

    let mut doc = Document::new()
        .set("xmlns", "http://www.w3.org/2000/svg")
        .set("xmlns:xlink", "http://www.w3.org/1999/xlink")
        .set("width", grid.width)
        .set("height", grid.height)
        .set("viewBox", format!("0 0 {} {}", grid.width, grid.height))
        .set("overflow", "visible");

    if bg.a > 0.0 {
        doc = doc.add(
            Rectangle::new()
                .set("width", "100%")
                .set("height", "100%")
                .set("fill", bg_css),
        );
    }

    doc = doc.add(grid.group);
    Ok(doc)
}

/// Render an aspect grid ("aspect table") for a single dataset as a composable `Group`.
///
/// Downstream apps can add the returned group to their own `svg::Document` and position/scale it
/// as needed.
pub fn aspect_grid_to_svg_group(
    theme: &Theme,
    aspect_rules: Option<&AspectRules>,
    data: &ChartData,
    opts: AspectGridSvgOptions<'_>,
) -> Result<AspectGridSvgGroup, ChartRenderError> {
    let placements = data
        .dataset_bodies(opts.dataset_id)
        .ok_or_else(|| {
            ChartRenderError::InvalidSpec(format!(
                "Aspect grid requested dataset '{}' but no such dataset exists",
                opts.dataset_id
            ))
        })?
        .to_vec();

    let cusps = data.house_set_cusps(opts.house_set_id).unwrap_or(&[]);

    // Filter + normalize into rows.
    let rules = aspect_rules.cloned().unwrap_or_default();

    let mut rows: Vec<GridRow> = Vec::new();
    for pm in &placements {
        let occ = pm.occupant();

        // Skip empty occupant placeholder entries.
        if matches!(occ, Occupant::Empty) {
            continue;
        }

        // Respect AspectRules endpoint inclusion switches.
        if !rules.endpoint_allowed(&occ) {
            continue;
        }

        let Some(sign_degree) = pm.coordinate().sign_degree() else {
            continue;
        };

        let endpoint_id = AspectEndpointId::from_occupant(occ);

        // Reduced-degree breakdown for display.
        let (sign, degree30) = sign_degree.sign_and_degree();
        let (deg_f, min_f, _sec_f) = degree30.nearest_degrees_minutes_seconds();

        let house_num = derive_house_number(cusps, sign_degree.degrees);

        rows.push(GridRow {
            occupant: occ,
            endpoint_id,
            sign,
            deg: deg_f as i32,
            min: min_f as i32,
            house_num,
        });
    }

    rows.sort_by_key(|r| occupant_sort_key(r.occupant));

    // Aspect computation uses the same placement list we used for rows.
    // We must re-build an ordered PlacementMotion list that aligns with row endpoint ids.
    let row_endpoint_set: BTreeMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.endpoint_id.0.clone(), i))
        .collect();

    let mut filtered_pms: Vec<rubrum::PlacementMotion> = Vec::new();
    for pm in &placements {
        let occ = pm.occupant();
        let id = AspectEndpointId::from_occupant(occ);
        if !row_endpoint_set.contains_key(id.0.as_str()) {
            continue;
        }
        if pm.coordinate().sign_degree().is_none() {
            continue;
        }
        filtered_pms.push(*pm);
    }

    let edges = compute_aspects_natal(filtered_pms.as_slice(), &rules);

    let mut aspect_map: BTreeMap<(String, String), DegreeAspectKind> = BTreeMap::new();
    for e in edges {
        aspect_map.insert(cell_id(&e.a, &e.b), e.kind);
    }

    let row_h = opts.row_height_px.unwrap_or(opts.cell_px);

    // Column widths.
    let icon_w = row_h;
    let label_w = (opts.cell_px * 7.5).max(160.0);
    let house_w = (opts.cell_px * 1.4).max(28.0);
    let gap = (opts.cell_px * 0.4).max(8.0);

    let n = rows.len();

    // The staircase matrix itself is strictly lower-triangular (no diagonal).
    // When `axis_labels` is enabled we also render diagonal endpoint glyphs, which
    // occupy the nth column (0..n-1).
    let matrix_cols = if opts.axis_labels {
        n
    } else {
        n.saturating_sub(1)
    };
    let matrix_w = (matrix_cols as f64) * opts.cell_px;
    let matrix_h = (n as f64) * row_h;

    let total_w = opts.margin_px * 2.0 + icon_w + label_w + house_w + gap + matrix_w;
    let total_h = opts.margin_px * 2.0 + matrix_h;

    let font_family = theme.cairo.font_family.as_str();

    let structure = aspect_grid_grid_line(theme);
    let structure_css = rgba_css_var("--rb-aspect-grid-grid-line", structure);

    let text_default = aspect_grid_text(theme);
    let text_default_css = rgba_css_var("--rb-aspect-grid-text", text_default);

    let cell_bg = aspect_grid_cell_bg(theme);
    let cell_bg_css = rgba_css_var_prim("--rb-aspect-grid-cell-bg", cell_bg);

    let x0 = opts.margin_px;
    let y0 = opts.margin_px;

    let matrix_x0 = x0 + icon_w + label_w + house_w + gap;

    let mut group = Group::new().set("id", "rb-aspect-grid");

    // Rows: left list + matrix.
    for (i, row) in rows.iter().enumerate() {
        let y = y0 + (i as f64) * row_h;

        let occupant_fallback = theme
            .dataset_colors
            .get(opts.dataset_id)
            .copied()
            .unwrap_or(text_default);
        let occupant_paint = resolve_occupant_glyph_paint(theme, row.occupant, occupant_fallback);
        let occ_color_css = glyph_color_css(occupant_paint, occupant_fallback);

        // Icon cell.
        group = group.add(rect_node(
            x0,
            y,
            icon_w,
            row_h,
            &cell_bg_css,
            &structure_css,
            1.0,
        ));

        let cx = x0 + icon_w / 2.0;
        let cy = y + row_h / 2.0;

        if let Some(href) = occupant_symbol_href(theme, row.occupant) {
            let size = opts.glyph_font_size_px * 1.6;
            let x_use = cx - size / 2.0;
            let y_use = cy - size / 2.0;

            let occupant_key_token = key_to_css_token(row.occupant.canonical_key());
            let occupant_type = rubrum_render::glyph_paint::occupant_type_key(row.occupant);
            let class = format!(
                "rb-ag-occupant rb-occupant-{} rb-occupant-type-{}",
                occupant_key_token,
                key_to_css_token(occupant_type)
            );
            let node = use_node(
                href.as_str(),
                x_use,
                y_use,
                size,
                class.as_str(),
                Some(occupant_paint),
            );
            group = group.add(node);
        } else {
            let sym = occupant_label(row.occupant);
            group = group.add(centered_text_node(
                cx,
                cy,
                sym.as_str(),
                &occ_color_css,
                font_family,
                opts.glyph_font_size_px,
                Some("rb-ag-occupant"),
            ));
        }

        // Label cell.
        let x_label = x0 + icon_w;
        group = group.add(rect_node(
            x_label,
            y,
            label_w,
            row_h,
            &cell_bg_css,
            &structure_css,
            1.0,
        ));

        // House cell.
        let x_house = x_label + label_w;
        group = group.add(rect_node(
            x_house,
            y,
            house_w,
            row_h,
            &cell_bg_css,
            &structure_css,
            1.0,
        ));

        // Text in label cell.
        let name = occupant_display_name(row.occupant);
        let deg_txt = format!("{:02}°{:02}'", row.deg, row.min);

        let tx = x_label + 6.0;
        let ty = y + row_h / 2.0;

        // Occupant name.
        group = group.add(left_text_node(
            tx,
            ty,
            name.as_str(),
            &occ_color_css,
            font_family,
            opts.font_size_px,
            Some("rb-ag-label"),
        ));

        // Sign glyph / symbol.
        let sign_x = tx + (opts.font_size_px * 4.5).max(56.0);
        if let Some(href) = sign_symbol_href(theme, row.sign) {
            let size = opts.font_size_px * 1.4;
            let x_use = sign_x;
            let y_use = ty - size / 2.0;

            let sign_paint = resolve_sign_glyph_paint(theme, row.sign, text_default);
            let sign_key_token = key_to_css_token(row.sign.canonical_key());
            let element_key_token = key_to_css_token(sign_element(row.sign).canonical_key());
            let class =
                format!("rb-ag-sign rb-sign-{sign_key_token} rb-sign-element-{element_key_token}");
            group = group.add(use_node(
                href.as_str(),
                x_use,
                y_use,
                size,
                class.as_str(),
                Some(sign_paint),
            ));

            // Degree text after sign glyph.
            let deg_x = x_use + size + 6.0;
            group = group.add(left_text_node(
                deg_x,
                ty,
                deg_txt.as_str(),
                &occ_color_css,
                font_family,
                opts.font_size_px,
                Some("rb-ag-deg"),
            ));
        } else {
            let sign_sym = row.sign.symbol_text();
            let fallback = format!("{} {}", sign_sym, deg_txt);
            group = group.add(left_text_node(
                sign_x,
                ty,
                fallback.as_str(),
                &occ_color_css,
                font_family,
                opts.font_size_px,
                Some("rb-ag-deg"),
            ));
        }

        // House number.
        if let Some(hn) = row.house_num {
            let hx = x_house + house_w / 2.0;
            let hy = y + row_h / 2.0;
            group = group.add(centered_text_node(
                hx,
                hy,
                hn.to_string().as_str(),
                &text_default_css,
                font_family,
                opts.font_size_px,
                Some("rb-ag-house"),
            ));
        }

        // Matrix cells for this row (stair-step: 0..i-1).
        for j in 0..i {
            let x = matrix_x0 + (j as f64) * opts.cell_px;

            group = group.add(rect_node(
                x,
                y,
                opts.cell_px,
                row_h,
                &cell_bg_css,
                &structure_css,
                1.0,
            ));

            let a = &row.endpoint_id;
            let b = &rows[j].endpoint_id;
            if let Some(kind) = aspect_map.get(&cell_id(a, b)).cloned() {
                let sym = kind.symbol_text().to_string();
                let c_css = rgba_css(aspect_kind_color(theme, &kind));

                let cx = x + opts.cell_px / 2.0;
                let cy = y + row_h / 2.0;
                let kind_class = format!("rb-ag-aspect rb-ag-aspect-{}", kind.to_string());

                group = group.add(centered_text_node(
                    cx,
                    cy,
                    sym.as_str(),
                    &c_css,
                    font_family,
                    opts.glyph_font_size_px,
                    Some(kind_class.to_lowercase().as_str()),
                ));
            }
        }

        // Right axis labels (row glyph), aligned to the end of the staircase.
        //
        // We include the first row (i = 0) so the top-most diagonal cell is present.
        if opts.axis_labels {
            let x = matrix_x0 + (i as f64) * opts.cell_px;
            group = group.add(rect_node(
                x,
                y,
                opts.cell_px,
                row_h,
                &cell_bg_css,
                &structure_css,
                1.0,
            ));

            let cx = x + opts.cell_px / 2.0;
            let cy = y + row_h / 2.0;

            if let Some(href) = occupant_symbol_href(theme, row.occupant) {
                let size = opts.glyph_font_size_px * 1.4;
                let x_use = cx - size / 2.0;
                let y_use = cy - size / 2.0;
                let occupant_key_token = key_to_css_token(row.occupant.canonical_key());
                let occupant_type = rubrum_render::glyph_paint::occupant_type_key(row.occupant);
                let class = format!(
                    "rb-ag-axis rb-occupant-{} rb-occupant-type-{}",
                    occupant_key_token,
                    key_to_css_token(occupant_type)
                );
                group = group.add(use_node(
                    href.as_str(),
                    x_use,
                    y_use,
                    size,
                    class.as_str(),
                    Some(occupant_paint),
                ));
            } else {
                let sym = occupant_label(row.occupant);
                group = group.add(centered_text_node(
                    cx,
                    cy,
                    sym.as_str(),
                    &occ_color_css,
                    font_family,
                    opts.glyph_font_size_px,
                    Some("rb-ag-axis"),
                ));
            }
        }
    }

    // No bottom axis labels: the left-hand placement list already labels endpoints.

    Ok(AspectGridSvgGroup {
        width: total_w,
        height: total_h,
        group,
    })
}

/// Render an aspect grid ("aspect table") for a single dataset.
///
/// This is a *non-wheel* render target intended for diagnostic tables / exports.
pub fn aspect_grid_to_svg_string(
    theme: &Theme,
    aspect_rules: Option<&AspectRules>,
    data: &ChartData,
    opts: AspectGridSvgOptions<'_>,
) -> Result<String, ChartRenderError> {
    Ok(aspect_grid_to_svg_document(theme, aspect_rules, data, opts)?.to_string())
}

/// Convenience helper: return the default occupant order used by the aspect grid.
///
/// This can be used by downstream apps to build allow/deny lists.
pub fn default_aspect_grid_endpoint_order() -> Vec<EndpointKey> {
    vec![
        EndpointKey::Body(rubrum::Body::Sun),
        EndpointKey::Body(rubrum::Body::Moon),
        EndpointKey::Body(rubrum::Body::Mercury),
        EndpointKey::Body(rubrum::Body::Venus),
        EndpointKey::Body(rubrum::Body::Mars),
        EndpointKey::Body(rubrum::Body::Jupiter),
        EndpointKey::Body(rubrum::Body::Saturn),
        EndpointKey::Body(rubrum::Body::Uranus),
        EndpointKey::Body(rubrum::Body::Neptune),
        EndpointKey::Body(rubrum::Body::Pluto),
        EndpointKey::ChartPoint(rubrum::ChartPoint::TrueNode),
        EndpointKey::ChartPoint(rubrum::ChartPoint::MeanApog),
        EndpointKey::Body(rubrum::Body::Chiron),
        EndpointKey::Lot(rubrum::Lot::Fortune),
        EndpointKey::Angle(rubrum::Angle::Vertex),
        EndpointKey::Angle(rubrum::Angle::Ascendant),
        EndpointKey::Angle(rubrum::Angle::Midheaven),
    ]
}
