use rubrum::{Occupant, OccupantFormat};
use rubrum_render::chart_data::ChartData;
use rubrum_render::declination_map::DeclinationMapLayout;
use rubrum_render::error::ChartRenderError;
use rubrum_render::glyph_paint::{
    resolve_occupant_glyph_paint, resolve_sign_glyph_paint, sign_element,
};
use rubrum_render::glyphs::{
    angle_svg_symbol_id, body_svg_symbol_id, chart_point_svg_symbol_id, lot_svg_symbol_id,
    sign_svg_symbol_id,
};
use rubrum_render::options::RgbaColor;
use rubrum_render::theme::Theme;
use svg::Document;
use svg::node::element::{Circle, Group, Line, Path, Rectangle, Text, Use};

use crate::primitive::{canonical_key_to_css_token as key_to_css_token, rgba_css_var};
use crate::spec::emit::glyph_paint_attrs;

#[derive(Debug, Clone)]
pub struct DeclinationMapSvgOptions<'a> {
    pub dataset_id: &'a str,
    pub glyph_sprite_url: Option<&'a str>,
    pub interactive_metadata: bool,
    pub title: Option<&'a str>,
}

impl Default for DeclinationMapSvgOptions<'_> {
    fn default() -> Self {
        Self {
            dataset_id: "natal",
            glyph_sprite_url: Some(""),
            interactive_metadata: true,
            title: Some("Declination map"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeclinationMapSvgGroup {
    pub width: f64,
    pub height: f64,
    pub group: Group,
}

#[derive(Clone, Debug)]
struct DeclinationPlacement {
    occupant: Occupant,
    longitude_deg: f64,
    declination_deg: f64,
    degree_label: String,
    is_angle: bool,
    out_of_bounds: bool,
}

fn normalize_deg(deg: f64) -> f64 {
    let mut out = deg % 360.0;
    if out < 0.0 {
        out += 360.0;
    }
    if out >= 360.0 {
        out -= 360.0;
    }
    out
}

fn x_for_longitude(layout: &DeclinationMapLayout, longitude_deg: f64) -> f64 {
    layout.margin_left + normalize_deg(longitude_deg) / 360.0 * layout.plot_width()
}

fn y_for_declination(layout: &DeclinationMapLayout, declination_deg: f64) -> f64 {
    let d = declination_deg.clamp(layout.min_declination_deg, layout.max_declination_deg);
    layout.margin_top
        + (layout.max_declination_deg - d)
            / (layout.max_declination_deg - layout.min_declination_deg).max(1.0)
            * layout.plot_height()
}

fn occupant_display_name(occupant: Occupant) -> String {
    occupant.format_occupant(OccupantFormat::Name)
}

fn occupant_symbol_label(occupant: Occupant) -> String {
    occupant.format_occupant(OccupantFormat::Symbol)
}

fn occupant_svg_symbol_id(occupant: Occupant) -> Option<String> {
    match occupant {
        Occupant::Empty => None,
        Occupant::Body(body) => Some(body_svg_symbol_id(body)),
        Occupant::Angle(angle) => Some(angle_svg_symbol_id(angle)),
        Occupant::ChartPoint(point) => Some(chart_point_svg_symbol_id(point)),
        Occupant::Lot(lot) => Some(lot_svg_symbol_id(lot)),
    }
}

fn sprite_href(sprite_url: Option<&str>, symbol_id: &str) -> String {
    match sprite_url {
        Some(url) if !url.is_empty() => format!("{url}#{symbol_id}"),
        _ => format!("#{symbol_id}"),
    }
}

fn longitude_degree_label(longitude_deg: f64) -> String {
    let lon = normalize_deg(longitude_deg);
    let in_sign = lon % 30.0;
    let deg = in_sign.floor() as u8;
    let min = ((in_sign - f64::from(deg)) * 60.0).round() as u8;
    if min >= 60 {
        format!("{:02}°", deg.saturating_add(1).min(29))
    } else {
        format!("{deg:02}°{min:02}′")
    }
}

fn declination_label(declination_deg: f64) -> String {
    let hemi = if declination_deg >= 0.0 { "N" } else { "S" };
    let abs = declination_deg.abs();
    let deg = abs.floor() as u8;
    let min = ((abs - f64::from(deg)) * 60.0).round() as u8;
    if min >= 60 {
        format!("{}°00′ {hemi}", deg.saturating_add(1))
    } else {
        format!("{deg}°{min:02}′ {hemi}")
    }
}

fn sign_name(idx: usize) -> &'static str {
    const SIGNS: [&str; 12] = [
        "Aries",
        "Taurus",
        "Gemini",
        "Cancer",
        "Leo",
        "Virgo",
        "Libra",
        "Scorpio",
        "Sagittarius",
        "Capricorn",
        "Aquarius",
        "Pisces",
    ];
    SIGNS[idx]
}

fn sign_by_index(idx: usize) -> rubrum::Sign {
    const SIGNS: [rubrum::Sign; 12] = [
        rubrum::Sign::Aries,
        rubrum::Sign::Taurus,
        rubrum::Sign::Gemini,
        rubrum::Sign::Cancer,
        rubrum::Sign::Leo,
        rubrum::Sign::Virgo,
        rubrum::Sign::Libra,
        rubrum::Sign::Scorpio,
        rubrum::Sign::Sagittarius,
        rubrum::Sign::Capricorn,
        rubrum::Sign::Aquarius,
        rubrum::Sign::Pisces,
    ];
    SIGNS[idx]
}

fn ecliptic_path_segments(layout: &DeclinationMapLayout) -> Vec<String> {
    // The ecliptic is a continuous sine-like curve. However, because the chart wraps
    // longitude at 0°/360°, tiny dashed fragments at the far left/right edge can read
    // as a stray dotted horizontal on the 0° declination line. Trim only those wrapped
    // edge fragments; keep the center crossing continuous.
    const EDGE_GAP_LONGITUDE_DEG: f64 = 3.0;

    let mut segments = Vec::new();
    let mut current = String::new();
    for i in 0..=288 {
        let longitude = i as f64 * 360.0 / 288.0;
        if longitude <= EDGE_GAP_LONGITUDE_DEG || longitude >= 360.0 - EDGE_GAP_LONGITUDE_DEG {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            continue;
        }

        let declination = layout.ecliptic_obliquity_deg * longitude.to_radians().sin();
        let x = x_for_longitude(layout, longitude);
        let y = y_for_declination(layout, declination);
        if current.is_empty() {
            current.push_str(&format!("M {x:.2} {y:.2}"));
        } else {
            current.push_str(&format!(" L {x:.2} {y:.2}"));
        }
    }

    if !current.is_empty() {
        segments.push(current);
    }

    segments
}

fn placements_from_data(
    data: &ChartData,
    dataset_id: &str,
    layout: &DeclinationMapLayout,
) -> Result<Vec<DeclinationPlacement>, ChartRenderError> {
    let bodies = data.dataset_bodies(dataset_id).ok_or_else(|| {
        ChartRenderError::InvalidSpec(format!(
            "Declination map requested dataset '{dataset_id}' but no such dataset exists"
        ))
    })?;

    Ok(bodies
        .iter()
        .filter_map(|pm| {
            let occupant = pm.occupant();
            if matches!(occupant, Occupant::Empty) {
                return None;
            }
            let sign_degree = pm.coordinate().sign_degree()?;
            let declination_deg = data
                .placement_metadata(dataset_id, occupant)
                .and_then(|m| m.declination_deg)?;
            Some(DeclinationPlacement {
                occupant,
                longitude_deg: sign_degree.degrees,
                declination_deg,
                degree_label: longitude_degree_label(sign_degree.degrees),
                is_angle: matches!(occupant, Occupant::Angle(_)),
                out_of_bounds: declination_deg.abs() > layout.ecliptic_obliquity_deg,
            })
        })
        .collect())
}

fn css_var(var: &str, fallback: RgbaColor) -> String {
    rgba_css_var(var, fallback)
}

fn declination_map_canvas_bg(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.canvas_bg)
        .unwrap_or_else(|| theme.effective_cairo_background())
}

fn declination_map_plot_bg(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.plot_bg)
        .unwrap_or_else(|| theme.effective_base_colors().muted)
}

fn declination_map_grid_line(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.grid_line)
        .unwrap_or_else(|| theme.effective_structure_color())
}

fn declination_map_equator(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.equator)
        .unwrap_or_else(|| theme.effective_structure_color())
}

fn declination_map_ecliptic(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.ecliptic)
        .unwrap_or_else(|| theme.effective_structure_color())
}

fn declination_map_tropic(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.tropic)
        .unwrap_or_else(|| theme.effective_structure_color())
}

fn declination_map_text(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.text)
        .unwrap_or_else(|| theme.effective_text_color())
}

fn declination_map_oob_band(theme: &Theme) -> RgbaColor {
    theme
        .svg
        .declination_map
        .and_then(|p| p.out_of_bounds_band)
        .unwrap_or_else(|| {
            let c = theme.effective_structure_color();
            RgbaColor { a: 0.08, ..c }
        })
}

fn apply_extra_attrs_to_group(mut group: Group, attrs: &str) -> Group {
    for (k, v) in crate::primitive::parse_extra_attrs(attrs) {
        group = group.set(k.as_ref(), v.as_ref());
    }
    group
}

fn apply_extra_attrs_to_use(mut node: Use, attrs: &str) -> Use {
    for (k, v) in crate::primitive::parse_extra_attrs(attrs) {
        node = node.set(k.as_ref(), v.as_ref());
    }
    node
}

/// Render a rectangular declination map as a standalone SVG document.
pub fn declination_map_to_svg_document(
    theme: &Theme,
    data: &ChartData,
    layout: &DeclinationMapLayout,
    opts: DeclinationMapSvgOptions<'_>,
) -> Result<Document, ChartRenderError> {
    let map = declination_map_to_svg_group(theme, data, layout, opts.clone())?;
    let bg = css_var(
        "--rb-declination-map-canvas-bg",
        declination_map_canvas_bg(theme),
    );
    let mut doc = Document::new()
        .set("xmlns", "http://www.w3.org/2000/svg")
        .set("xmlns:xlink", "http://www.w3.org/1999/xlink")
        .set("width", map.width)
        .set("height", map.height)
        .set("viewBox", format!("0 0 {} {}", map.width, map.height))
        .set("overflow", "visible")
        .set("class", "rb-declination-map-svg");

    doc = doc.add(
        Rectangle::new()
            .set("width", "100%")
            .set("height", "100%")
            .set("rx", 18)
            .set("fill", bg),
    );
    Ok(doc.add(map.group))
}

/// Render a rectangular declination map as a composable SVG group.
pub fn declination_map_to_svg_group(
    theme: &Theme,
    data: &ChartData,
    layout: &DeclinationMapLayout,
    opts: DeclinationMapSvgOptions<'_>,
) -> Result<DeclinationMapSvgGroup, ChartRenderError> {
    let placements = placements_from_data(data, opts.dataset_id, layout)?;
    let plot_w = layout.plot_width();
    let plot_h = layout.plot_height();
    let plot_x2 = layout.margin_left + plot_w;
    let plot_y2 = layout.margin_top + plot_h;
    let text = declination_map_text(theme);
    let grid = declination_map_grid_line(theme);
    let equator = declination_map_equator(theme);
    let ecliptic = declination_map_ecliptic(theme);
    let tropic = declination_map_tropic(theme);
    let plot_bg = declination_map_plot_bg(theme);
    let oob_band = declination_map_oob_band(theme);
    let font = theme.cairo.font_family.clone();

    let mut group = Group::new().set("class", "rb-declination-map");
    if let Some(title) = opts.title {
        group = group.add(Text::new(title.to_string()).set("class", "rb-declination-map-title"));
    }

    group = group.add(
        Rectangle::new()
            .set("class", "rb-declination-map-plot-bg")
            .set("x", layout.margin_left)
            .set("y", layout.margin_top)
            .set("width", plot_w)
            .set("height", plot_h)
            .set("rx", 10)
            .set("fill", css_var("--rb-declination-map-plot-bg", plot_bg))
            .set("stroke", css_var("--rb-declination-map-grid", grid))
            .set("stroke-opacity", 0.22),
    );

    if layout.show_out_of_bounds_bands {
        for d in [
            layout.ecliptic_obliquity_deg,
            -layout.ecliptic_obliquity_deg,
        ] {
            let y = y_for_declination(layout, d);
            let (band_y, band_h) = if d > 0.0 {
                (layout.margin_top, (y - layout.margin_top).max(0.0))
            } else {
                (y, (plot_y2 - y).max(0.0))
            };
            group = group.add(
                Rectangle::new()
                    .set("class", "rb-declination-map-oob-band")
                    .set("x", layout.margin_left)
                    .set("y", band_y)
                    .set("width", plot_w)
                    .set("height", band_h)
                    .set("fill", css_var("--rb-declination-map-oob-band", oob_band)),
            );
        }
    }

    if layout.show_sign_blocks {
        for idx in 0..12 {
            let x = layout.margin_left + idx as f64 * plot_w / 12.0;
            let sign = sign_by_index(idx);
            let sign_token = key_to_css_token(sign.canonical_key());
            let element_token = key_to_css_token(sign_element(sign).canonical_key());
            group = group.add(
                Rectangle::new()
                    .set(
                        "class",
                        format!("rb-declination-map-sign-block rb-sign-{sign_token} rb-sign-element-{element_token}"),
                    )
                    .set("x", x)
                    .set("y", layout.margin_top)
                    .set("width", plot_w / 12.0)
                    .set("height", plot_h)
                    .set("fill", if idx % 2 == 0 { "transparent" } else { "color-mix(in srgb, var(--rb-declination-map-grid) 4%, transparent)" }),
            );
        }
    }

    for idx in 0..=12 {
        let x = layout.margin_left + idx as f64 * plot_w / 12.0;
        group = group.add(
            Line::new()
                .set(
                    "class",
                    "rb-declination-map-grid-line rb-declination-map-grid-line-longitude",
                )
                .set("x1", x)
                .set("y1", layout.margin_top)
                .set("x2", x)
                .set("y2", plot_y2)
                .set("stroke", css_var("--rb-declination-map-grid", grid))
                .set("stroke-opacity", 0.20),
        );
    }

    if layout.show_ecliptic_curve {
        for path in ecliptic_path_segments(layout) {
            group = group.add(
                Path::new()
                    .set("class", "rb-declination-map-ecliptic")
                    .set("d", path)
                    .set("fill", "none")
                    .set("stroke", css_var("--rb-declination-map-ecliptic", ecliptic))
                    .set("stroke-width", 2.4)
                    .set("stroke-linecap", "round")
                    .set("stroke-linejoin", "round")
                    .set("stroke-dasharray", "9 7")
                    .set("stroke-opacity", 0.78),
            );
        }
    }

    for d in [-30, -20, -10, 0, 10, 20, 30] {
        let y = y_for_declination(layout, d as f64);
        let is_equator = d == 0;
        group = group.add(
            Line::new()
                .set(
                    "class",
                    if is_equator {
                        "rb-declination-map-equator"
                    } else {
                        "rb-declination-map-grid-line rb-declination-map-grid-line-declination"
                    },
                )
                .set("x1", layout.margin_left)
                .set("y1", y)
                .set("x2", plot_x2)
                .set("y2", y)
                .set(
                    "stroke",
                    if is_equator {
                        css_var("--rb-declination-map-equator", equator)
                    } else {
                        css_var("--rb-declination-map-grid", grid)
                    },
                )
                .set("stroke-width", if is_equator { 2.2 } else { 1.0 })
                .set("stroke-dasharray", "none")
                .set("stroke-opacity", if is_equator { 1.0 } else { 0.20 }),
        );
        let label = if d > 0 {
            format!("+{d}°")
        } else {
            format!("{d}°")
        };
        group = group.add(
            Text::new(label)
                .set("class", "rb-declination-map-axis-label")
                .set("x", layout.margin_left - 16.0)
                .set("y", y + 4.0)
                .set("text-anchor", "end")
                .set("font-family", font.as_str())
                .set("font-size", 12)
                .set("font-weight", 760)
                .set("fill", css_var("--rb-declination-map-text", text)),
        );
    }

    if layout.show_tropics {
        for d in [
            layout.ecliptic_obliquity_deg,
            -layout.ecliptic_obliquity_deg,
        ] {
            let y = y_for_declination(layout, d);
            group = group.add(
                Line::new()
                    .set("class", "rb-declination-map-tropic")
                    .set("x1", layout.margin_left)
                    .set("y1", y)
                    .set("x2", plot_x2)
                    .set("y2", y)
                    .set("stroke", css_var("--rb-declination-map-tropic", tropic))
                    .set("stroke-opacity", 0.42),
            );
        }
    }

    for idx in 0..12 {
        let sign = sign_by_index(idx);
        let x = layout.margin_left + (idx as f64 + 0.5) * plot_w / 12.0;
        let y = plot_y2 + 30.0;
        let href = sprite_href(opts.glyph_sprite_url, sign_svg_symbol_id(sign));
        let sign_token = key_to_css_token(sign.canonical_key());
        let element_token = key_to_css_token(sign_element(sign).canonical_key());
        let sign_paint_attrs = glyph_paint_attrs(resolve_sign_glyph_paint(theme, sign, text));
        let sign_group = apply_extra_attrs_to_group(
            Group::new()
                .set(
                    "class",
                    format!("rb-declination-map-sign-label rb-sign-{sign_token} rb-sign-element-{element_token}"),
                )
                .set("transform", format!("translate({x:.2} {y:.2})")),
            sign_paint_attrs.as_str(),
        )
            .add(
                apply_extra_attrs_to_use(
                    Use::new()
                        .set("class", "rb-declination-map-sign-glyph")
                        .set("href", href.as_str())
                        .set("xlink:href", href.as_str())
                        .set("x", -7)
                        .set("y", -14)
                        .set("width", 14)
                        .set("height", 14)
                        .set("color", "currentColor"),
                    sign_paint_attrs.as_str(),
                ),
            )
            .add(
                Text::new(sign_name(idx).to_string())
                    .set("y", 14)
                    .set("text-anchor", "middle")
                    .set("fill", css_var("--rb-declination-map-text", text))
                    .set("font-family", font.as_str())
                    .set("font-size", 12)
                    .set("font-weight", 760),
            );
        group = group.add(sign_group);
    }

    if layout.show_angle_guides {
        for p in placements.iter().filter(|p| p.is_angle) {
            let x = x_for_longitude(layout, p.longitude_deg);
            let y = y_for_declination(layout, p.declination_deg);
            group = group.add(
                Group::new()
                    .set("class", "rb-declination-map-angle-guide")
                    .add(
                        Line::new()
                            .set("x1", x)
                            .set("y1", layout.margin_top)
                            .set("x2", x)
                            .set("y2", plot_y2)
                            .set("stroke", css_var("--rb-declination-map-grid", grid))
                            .set("stroke-opacity", 0.45)
                            .set("stroke-width", 2)
                            .set("stroke-dasharray", "5 7"),
                    )
                    .add(
                        Text::new(occupant_symbol_label(p.occupant))
                            .set("x", x)
                            .set("y", layout.margin_top + 16.0)
                            .set("text-anchor", "middle")
                            .set("fill", css_var("--rb-declination-map-text", text))
                            .set("font-family", font.as_str())
                            .set("font-size", 13)
                            .set("font-weight", 900),
                    )
                    .add(
                        Circle::new()
                            .set("cx", x)
                            .set("cy", y)
                            .set("r", 4)
                            .set("fill", css_var("--rb-declination-map-equator", equator)),
                    ),
            );
        }
    }

    let fallback_color = theme.effective_text_color();
    for (idx, p) in placements.into_iter().enumerate() {
        let x = x_for_longitude(layout, p.longitude_deg);
        let y = y_for_declination(layout, p.declination_deg);
        let occupant_key = p.occupant.canonical_key();
        let occupant_token = key_to_css_token(occupant_key);
        let occupant_type = rubrum_render::glyph_paint::occupant_type_key(p.occupant);
        let type_token = key_to_css_token(occupant_type);
        let mut class = format!(
            "rb-declination-map-placement rb-placement rb-placement-{occupant_token} rb-occupant-{occupant_token} rb-occupant-type-{type_token}"
        );
        if p.out_of_bounds {
            class.push_str(" rb-declination-map-placement-oob");
        }
        if p.is_angle {
            class.push_str(" rb-declination-map-placement-angle");
        }

        let label_dx = if idx % 2 == 0 { 14.0 } else { -14.0 };
        let anchor = if idx % 2 == 0 { "start" } else { "end" };
        let label_y = if idx % 3 == 0 { -10.0 } else { 14.0 };
        let title = format!(
            "{}: {} longitude, {} declination",
            occupant_display_name(p.occupant),
            p.degree_label,
            declination_label(p.declination_deg)
        );
        let placement_paint = resolve_occupant_glyph_paint(theme, p.occupant, fallback_color);
        let placement_paint_attrs = glyph_paint_attrs(placement_paint);
        let mut placement_group = apply_extra_attrs_to_group(
            Group::new()
                .set("class", class)
                .set("transform", format!("translate({x:.2} {y:.2})")),
            placement_paint_attrs.as_str(),
        );
        if opts.interactive_metadata {
            placement_group = placement_group
                .set("data-rb-dataset", opts.dataset_id)
                .set("data-rb-occupant", occupant_key)
                .set(
                    "data-rb-endpoint",
                    format!("{}:{}", opts.dataset_id, occupant_key),
                )
                .set("data-rb-occupant-type", occupant_type)
                .set("data-rb-degree", p.longitude_deg)
                .set("data-rb-declination", p.declination_deg);
        }

        placement_group = placement_group
            .add(svg::node::element::Title::new(title))
            .add(
                Circle::new()
                    .set("class", "rb-declination-map-placement-halo")
                    .set("cx", 0)
                    .set("cy", 0)
                    .set("r", if p.out_of_bounds { 14 } else { 11 }),
            );

        if let Some(symbol_id) = occupant_svg_symbol_id(p.occupant) {
            let href = sprite_href(opts.glyph_sprite_url, &symbol_id);
            let paint = placement_paint;
            let attrs = glyph_paint_attrs(paint);
            let glyph = apply_extra_attrs_to_use(
                Use::new()
                    .set("class", "rb-declination-map-glyph rb-occupant-glyph")
                    .set("href", href.as_str())
                    .set("xlink:href", href.as_str())
                    .set("x", -8)
                    .set("y", -8)
                    .set("width", 16)
                    .set("height", 16)
                    .set("color", "currentColor"),
                attrs.as_str(),
            );
            placement_group = placement_group.add(glyph);
        } else {
            placement_group = placement_group.add(
                Text::new(occupant_symbol_label(p.occupant))
                    .set("class", "rb-declination-map-fallback-glyph")
                    .set("text-anchor", "middle")
                    .set("dominant-baseline", "central"),
            );
        }

        if layout.show_degree_labels {
            placement_group = placement_group.add(
                Text::new("")
                    .set("class", "rb-declination-map-placement-label")
                    .set("x", label_dx)
                    .set("y", label_y)
                    .set("text-anchor", anchor)
                    .set("font-family", font.as_str())
                    .set("font-size", 11)
                    .set("font-weight", 850)
                    .add(
                        svg::node::element::TSpan::new(p.degree_label)
                            .set("class", "rb-declination-map-placement-degree"),
                    ),
            );
        }

        group = group.add(placement_group);
    }

    Ok(DeclinationMapSvgGroup {
        width: layout.width,
        height: layout.height,
        group,
    })
}

/// Render a rectangular declination map as a standalone SVG string.
pub fn declination_map_to_svg_string(
    theme: &Theme,
    data: &ChartData,
    layout: &DeclinationMapLayout,
    opts: DeclinationMapSvgOptions<'_>,
) -> Result<String, ChartRenderError> {
    Ok(declination_map_to_svg_document(theme, data, layout, opts)?.to_string())
}
