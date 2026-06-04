use crate::primitive::{canonical_key_to_css_token as key_to_css_token, escape_xml_attr};
use rubrum::{House, Occupant};
use rubrum_render::core::geometry::{normalize_deg, polar_to_xy};
use rubrum_render::glyph_paint::{
    GlyphPaint, resolve_occupant_glyph_paint, resolve_sign_glyph_paint, sign_element,
};
use rubrum_render::glyphs::{
    angle_svg_symbol_id, body_svg_symbol_id, chart_point_svg_symbol_id, lot_svg_symbol_id,
    occupant_label, retrograde_svg_symbol_id, sign_svg_symbol_id,
};
use rubrum_render::labels::render_placement_label_template;
use rubrum_render::layout::{
    GlyphLaneMode, LaneSpec, PlacementLabelSegmentInput, PlacementLabelSegmentSpec,
    PlacementLabelsSpec, PlacementSignGlyphSpec,
};
use rubrum_render::options::RgbaColor;
use rubrum_render::style::resolve_lane_style;
use rubrum_render::theme::Theme;
use rubrum_render::{chart_data::ChartData, error::ChartRenderError};

use super::emit::{glyph_paint_attrs, push_hit_circle, push_text, push_text_extra, push_use};

fn segment_spec_from_input(
    seg: &PlacementLabelSegmentInput,
) -> (String, Option<PlacementLabelSegmentSpec>) {
    match seg {
        PlacementLabelSegmentInput::Text(s) => (s.clone(), None),
        PlacementLabelSegmentInput::Spec(spec) => (spec.text.clone(), Some(spec.clone())),
    }
}

fn resolve_labels_text_style(
    labels: &PlacementLabelsSpec,
    segment_spec: Option<&PlacementLabelSegmentSpec>,
    fallback_font_family: &str,
    fallback_font_size: f64,
    fallback_color: RgbaColor,
) -> (String, f64, RgbaColor) {
    let mut font_family = labels
        .text
        .as_ref()
        .and_then(|t| t.font_family.clone())
        .unwrap_or_else(|| fallback_font_family.to_owned());

    let mut font_size = labels
        .text
        .as_ref()
        .and_then(|t| t.font_size)
        .or(labels.font_size)
        .unwrap_or(fallback_font_size)
        .max(1.0);

    let mut color = labels
        .text
        .as_ref()
        .and_then(|t| t.color)
        .or(labels.color)
        .unwrap_or(fallback_color);

    if let Some(spec) = segment_spec {
        if let Some(ff) = spec.font_family.as_ref() {
            font_family = ff.clone();
        }
        if let Some(fs) = spec.font_size {
            font_size = fs.max(1.0);
        }
        if let Some(c) = spec.color {
            color = c;
        }
    }

    (font_family, font_size, color)
}

fn resolve_sign_glyph_style(
    labels: &PlacementLabelsSpec,
    segment_spec: Option<&PlacementLabelSegmentSpec>,
) -> Option<PlacementSignGlyphSpec> {
    segment_spec
        .and_then(|s| s.sign_glyph.clone())
        .or_else(|| labels.sign_glyph.clone())
}

fn estimate_text_bbox_size(text: &str, font_size: f64) -> f64 {
    // Best-effort bbox estimate for collision avoidance.
    //
    // We use the maximum of width/height as a conservative scalar size.
    let char_count = text.chars().count().max(1) as f64;
    let w = char_count * font_size * 0.6;
    let h = font_size * 1.2;
    w.max(h).max(1.0)
}

fn estimate_sign_bbox_size(symbol_size: f64) -> f64 {
    symbol_size.max(1.0)
}

#[allow(clippy::too_many_arguments)]
fn push_retrograde_marker(
    out: &mut String,
    sprite_url: Option<&str>,
    x: f64,
    y: f64,
    occupant_size: f64,
    text_color: RgbaColor,
    font_family: &str,
    font_size: f64,
) {
    let marker_size = (occupant_size * 0.48).max(6.0);
    let marker_offset = occupant_size * 0.42;
    let marker_x = x + marker_offset;
    let marker_y = y + marker_offset;

    if let Some(sprite_base) = sprite_url {
        let href = format!("{sprite_base}#{}", retrograde_svg_symbol_id());
        let paint_attrs = glyph_paint_attrs(GlyphPaint::monochrome(text_color));
        let extra = format!("data-rb-motion=\"retrograde\" {paint_attrs}");

        push_use(
            out,
            href.as_str(),
            marker_x,
            marker_y,
            marker_size,
            "rb-motion rb-motion-retrograde rb-motion-retrograde-glyph",
            Some(extra.as_str()),
        );
    } else {
        push_text_extra(
            out,
            marker_x,
            marker_y,
            "℞",
            text_color,
            font_family,
            (font_size * 0.62).max(6.0),
            Some("rb-motion rb-motion-retrograde rb-motion-retrograde-text"),
            Some("data-rb-motion=\"retrograde\""),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_lane_glyphs(
    out: &mut String,
    theme: &Theme,
    lane: &LaneSpec,
    data: &ChartData,
    cx: f64,
    cy: f64,
    lane_r_inner: f64,
    lane_r_outer: f64,
    rotation_deg: f64,
    default_text_color: RgbaColor,
) -> Result<(), ChartRenderError> {
    let options = &theme.cairo;

    let Some(glyphs) = lane.glyphs.as_ref() else {
        return Ok(());
    };

    let radial_bias = glyphs.radial_bias.unwrap_or(0.5).clamp(0.0, 1.0);
    let glyph_r = lane_r_inner + (lane_r_outer - lane_r_inner) * radial_bias;

    let style = resolve_lane_style(theme, lane);

    match glyphs.mode {
        GlyphLaneMode::Aspects | GlyphLaneMode::CrossAspects => Ok(()),
        GlyphLaneMode::HouseNumbers => {
            let house_set_id = lane.house_set.as_deref().unwrap_or("natal");
            let cusps = data.house_set_cusps(house_set_id).ok_or_else(|| {
                ChartRenderError::InvalidSpec(format!("Unknown house_set '{house_set_id}'"))
            })?;

            if cusps.len() < 12 {
                return Ok(());
            }

            let font_family = style
                .font_family
                .as_deref()
                .unwrap_or(options.font_family.as_str());

            let font_size = style
                .font_size
                .unwrap_or((options.label_font_size * 0.8).max(10.0));

            let color = style.stroke.unwrap_or(default_text_color);

            let houses = House::default_order();
            for (idx, house) in houses.iter().copied().enumerate() {
                let Some(cusp_deg) = cusps
                    .iter()
                    .find(|c| c.house == house)
                    .map(|c| c.sign_degree.degrees)
                else {
                    continue;
                };

                let next_house = houses[(idx + 1) % houses.len()];
                let next_cusp_deg = cusps
                    .iter()
                    .find(|c| c.house == next_house)
                    .map(|c| c.sign_degree.degrees)
                    .unwrap_or(cusp_deg + 30.0);

                let delta = normalize_deg(next_cusp_deg - cusp_deg);
                let mid_abs = cusp_deg + (delta / 2.0);
                let mid_lon_deg = normalize_deg(mid_abs + rotation_deg);

                let (x, y) = polar_to_xy(cx, cy, glyph_r, mid_lon_deg);
                let text = house.to_1_based_i32().to_string();
                push_text(out, x, y, text.as_str(), color, font_family, font_size);
            }

            Ok(())
        }
        GlyphLaneMode::Bodies => {
            let Some(dataset) = lane.dataset.as_ref() else {
                return Ok(());
            };

            let bodies = data.dataset_bodies(dataset).ok_or_else(|| {
                ChartRenderError::InvalidSpec(format!("Unknown dataset '{dataset}'"))
            })?;

            let dataset_color = theme.dataset_colors.get(dataset).copied();

            let font_family = style
                .font_family
                .as_deref()
                .unwrap_or(options.font_family.as_str());

            let font_size = style.font_size.unwrap_or(options.label_font_size);

            let text_color = style.stroke.or(dataset_color).unwrap_or(default_text_color);

            let symbol_size = theme.cairo.occupant_symbol_size.max(1.0);
            let sprite_url = theme.svg.glyph_sprite_url.as_deref();

            let endpoint_filter = lane.endpoint_filter.as_ref().map(|f| f.compile());

            // Placement labels (segments near the glyph).
            let labels_spec = glyphs
                .placement_labels
                .as_ref()
                .filter(|s| s.enabled && !s.segments.is_empty());

            for pm in bodies.iter().copied() {
                if let Some(filter) = endpoint_filter.as_ref() {
                    if !filter.endpoint_allowed(pm.occupant()) {
                        continue;
                    }
                }

                let Some(sign_degree) = pm.coordinate().sign_degree() else {
                    continue;
                };

                let lon_deg = normalize_deg(sign_degree.degrees + rotation_deg);
                let (x, y) = polar_to_xy(cx, cy, glyph_r, lon_deg);

                // If a sprite sheet is configured and we can map this occupant to a stable symbol ID,
                // render via <use href="{sprite}#rb-body-sun">. Otherwise fall back to text.
                let sprite_href = sprite_url.and_then(|base| {
                    let symbol_id = match pm.occupant() {
                        Occupant::Body(body) => Some(body_svg_symbol_id(body)),
                        Occupant::ChartPoint(point) => Some(chart_point_svg_symbol_id(point)),
                        Occupant::Angle(angle) => Some(angle_svg_symbol_id(angle)),
                        Occupant::Lot(lot) => Some(lot_svg_symbol_id(lot)),
                        _ => None,
                    }?;
                    Some(format!("{base}#{symbol_id}"))
                });

                let dataset_attr = escape_xml_attr(dataset.as_str());

                let occupant = pm.occupant();
                let occupant_key = occupant.canonical_key();
                let occupant_key_attr = escape_xml_attr(occupant_key);
                let occupant_key_token = key_to_css_token(occupant_key);

                let occupant_type = match occupant {
                    Occupant::Empty => "empty",
                    Occupant::Body(_) => "body",
                    Occupant::ChartPoint(_) => "chart-point",
                    Occupant::Angle(_) => "angle",
                    Occupant::Lot(_) => "lot",
                };

                let retro_attr = if pm.is_retrograde() { "true" } else { "false" };

                // Wrap the placement in a group with stable metadata for browser hit-testing.
                // This allows the Trunk/WASM example to identify which placement was clicked.
                out.push_str(&format!(
                    "  <g class=\"rb-placement rb-placement-{occupant_key_token}\" data-rb-dataset=\"{dataset_attr}\" data-rb-endpoint=\"{occupant_key_attr}\" data-rb-occupant=\"{occupant_key_attr}\" data-rb-occupant-type=\"{occupant_type}\" data-rb-degree=\"{}\" data-rb-retrograde=\"{retro_attr}\">\n",
                    sign_degree.degrees
                ));

                // Increase clickable area: add an invisible hit target behind the glyph/text.
                // The click handler will walk up to the parent <g> to read data-rb-* attributes.
                let hit_r = if sprite_href.is_some() {
                    (symbol_size * 0.75).max(14.0)
                } else {
                    (font_size * 0.9).max(14.0)
                };
                push_hit_circle(out, x, y, hit_r, "rb-placement-hit");

                if let Some(href) = sprite_href.as_deref() {
                    let paint = resolve_occupant_glyph_paint(theme, occupant, text_color);
                    let paint_attrs = glyph_paint_attrs(paint);
                    let use_extra = format!(
                        "data-rb-endpoint=\"{occupant_key_attr}\" data-rb-occupant=\"{occupant_key_attr}\" data-rb-occupant-type=\"{occupant_type}\" {paint_attrs}"
                    );
                    let class = format!(
                        "rb-occupant rb-occupant-glyph rb-occupant-{occupant_key_token} rb-occupant-type-{occupant_type}"
                    );
                    push_use(
                        out,
                        href,
                        x,
                        y,
                        symbol_size,
                        class.as_str(),
                        Some(use_extra.as_str()),
                    );
                } else {
                    let label = occupant_label(occupant);

                    push_text(
                        out,
                        x,
                        y,
                        label.as_str(),
                        text_color,
                        font_family,
                        font_size,
                    );
                }

                if pm.is_retrograde() {
                    let occupant_size = if sprite_href.is_some() {
                        symbol_size
                    } else {
                        font_size
                    };
                    push_retrograde_marker(
                        out,
                        sprite_url,
                        x,
                        y,
                        occupant_size,
                        text_color,
                        font_family,
                        font_size,
                    );
                }

                // Optional per-placement label segments rendered near the glyph.
                if let Some(labels_spec) = labels_spec {
                    let (sign, degree30) = sign_degree.sign_and_degree();
                    let (deg_f, min_f, sec_f) = degree30.nearest_degrees_minutes_seconds();

                    // `nearest_degrees_minutes_seconds()` returns bounded (integer-ish) components.
                    let deg = deg_f as i32;
                    let min = min_f as i32;
                    let sec = sec_f as i32;

                    let (global_font_family, global_font_size, global_color) =
                        resolve_labels_text_style(
                            labels_spec,
                            None,
                            font_family,
                            font_size,
                            text_color,
                        );

                    let label_side = labels_spec
                        .side
                        .unwrap_or(rubrum_render::layout::PlacementLabelSide::Inner);

                    // By default, place labels just inside the glyph and step inward by roughly one line.
                    // For `Outer` labels, we interpret the same config as an outward offset from the glyph.
                    let base_offset_in = labels_spec
                        .offset_in
                        .unwrap_or((options.label_font_size * 0.9).max(10.0));

                    let step_in = labels_spec.step_in.unwrap_or(global_font_size * 1.15);

                    let label_collision_avoidance =
                        labels_spec.collision_avoidance.unwrap_or(false);

                    // Track the last segment's rendered radius and approximate bounding-box size so we can
                    // push subsequent segments further inward if they would overlap.
                    let mut prev_r: Option<f64> = None;
                    let mut prev_bbox_size: f64 = 0.0;

                    out.push_str("    <g class=\"rb-placement-labels\">\n");

                    for (j, segment_input) in labels_spec.segments.iter().enumerate() {
                        let (segment_tpl, segment_spec) = segment_spec_from_input(segment_input);

                        let (seg_font_family, seg_font_size, seg_color) = resolve_labels_text_style(
                            labels_spec,
                            segment_spec.as_ref(),
                            global_font_family.as_str(),
                            global_font_size,
                            global_color,
                        );

                        let baseline_offset_in = segment_spec
                            .as_ref()
                            .and_then(|s| s.offset_in)
                            .or_else(|| {
                                labels_spec
                                    .segment_offsets_in
                                    .as_ref()
                                    .and_then(|o| o.get(j).copied())
                            })
                            .unwrap_or(base_offset_in + (j as f64) * step_in);

                        // Special-case glyph segments.
                        //
                        // Convention: if the segment is exactly "{sign_glyph}", we attempt sprite glyph injection.
                        // Otherwise we treat "{sign_glyph}" as a text token that becomes the unicode sign.
                        let is_sign_glyph_segment = segment_tpl.trim() == "{sign_glyph}";

                        let mut segment_bbox_size = 0.0;

                        let label_text = if is_sign_glyph_segment {
                            None
                        } else {
                            Some(render_placement_label_template(
                                segment_tpl.as_str(),
                                sign,
                                deg,
                                min,
                                sec,
                            ))
                        };

                        if is_sign_glyph_segment {
                            let glyph_style =
                                resolve_sign_glyph_style(labels_spec, segment_spec.as_ref());

                            let derived_size = seg_font_size * 1.6;
                            let size = glyph_style
                                .as_ref()
                                .and_then(|s| s.size)
                                .unwrap_or(derived_size)
                                .max(1.0);

                            segment_bbox_size = estimate_sign_bbox_size(size);
                        } else if let Some(label_text) = label_text.as_ref() {
                            segment_bbox_size =
                                estimate_text_bbox_size(label_text.as_str(), seg_font_size);
                        }

                        // Start with the baseline config-derived offset, and only push further towards the chosen side.
                        //
                        // - Inner: larger offsets move labels inward (smaller radius)
                        // - Outer: larger offsets move labels outward (larger radius)
                        let mut offset_in = baseline_offset_in;

                        if label_collision_avoidance {
                            if let Some(prev_r) = prev_r {
                                let this_r = match label_side {
                                    rubrum_render::layout::PlacementLabelSide::Inner => {
                                        (glyph_r - offset_in).max(0.0)
                                    }
                                    rubrum_render::layout::PlacementLabelSide::Outer => {
                                        glyph_r + offset_in
                                    }
                                };

                                // Ensure at least half-bbox + half-bbox spacing between segment centers.
                                // This is a conservative approximation since text is not rotated.
                                let padding = (seg_font_size * 0.15).max(2.0);
                                let needed =
                                    (prev_bbox_size / 2.0) + (segment_bbox_size / 2.0) + padding;

                                match label_side {
                                    rubrum_render::layout::PlacementLabelSide::Inner => {
                                        if (prev_r - this_r) < needed {
                                            let forced_r = (prev_r - needed).max(0.0);
                                            offset_in = (glyph_r - forced_r).max(offset_in);
                                        }
                                    }
                                    rubrum_render::layout::PlacementLabelSide::Outer => {
                                        if (this_r - prev_r) < needed {
                                            let forced_r = prev_r + needed;
                                            offset_in = (forced_r - glyph_r).max(offset_in);
                                        }
                                    }
                                }
                            }
                        }

                        let r = match label_side {
                            rubrum_render::layout::PlacementLabelSide::Inner => {
                                (glyph_r - offset_in).max(0.0)
                            }
                            rubrum_render::layout::PlacementLabelSide::Outer => glyph_r + offset_in,
                        };

                        if matches!(label_side, rubrum_render::layout::PlacementLabelSide::Inner)
                            && r <= 0.0
                        {
                            break;
                        }

                        let (lx, ly) = polar_to_xy(cx, cy, r, lon_deg);

                        if is_sign_glyph_segment {
                            let glyph_style =
                                resolve_sign_glyph_style(labels_spec, segment_spec.as_ref());

                            let derived_size = seg_font_size * 1.6;
                            let size = glyph_style
                                .as_ref()
                                .and_then(|s| s.size)
                                .unwrap_or(derived_size)
                                .max(1.0);

                            let mut paint = resolve_sign_glyph_paint(theme, sign, seg_color);
                            if let Some(glyph_style) = glyph_style.as_ref()
                                && let Some(color) = glyph_style.color
                            {
                                paint = GlyphPaint::monochrome(color).overlay(paint);
                            }

                            if let Some(sprite_base) = sprite_url {
                                let symbol_id = sign_svg_symbol_id(sign);
                                let href = format!("{sprite_base}#{symbol_id}");

                                let sign_key = sign.canonical_key();
                                let sign_key_token = key_to_css_token(sign_key);
                                let element = sign_element(sign);
                                let element_key = element.canonical_key();
                                let element_key_token = key_to_css_token(element_key);
                                let paint_attrs = glyph_paint_attrs(paint);
                                let extra = format!(
                                    "data-rb-label-seg=\"{j}\" data-rb-label-kind=\"sign-glyph\" data-rb-sign=\"{}\" data-rb-sign-element=\"{}\" {}",
                                    escape_xml_attr(sign_key),
                                    escape_xml_attr(element_key),
                                    paint_attrs
                                );
                                let class = format!(
                                    "rb-placement-label rb-placement-label-sign-glyph rb-sign-{sign_key_token} rb-sign-element-{element_key_token}"
                                );

                                push_use(
                                    out,
                                    href.as_str(),
                                    lx,
                                    ly,
                                    size,
                                    class.as_str(),
                                    Some(extra.as_str()),
                                );
                            } else {
                                let glyph_color = paint.color.unwrap_or(seg_color);
                                push_text(
                                    out,
                                    lx,
                                    ly,
                                    sign.symbol_text().as_str(),
                                    glyph_color,
                                    seg_font_family.as_str(),
                                    seg_font_size,
                                );
                            }
                        } else {
                            let label_text = label_text.as_deref().unwrap_or_default();
                            push_text(
                                out,
                                lx,
                                ly,
                                label_text,
                                seg_color,
                                seg_font_family.as_str(),
                                seg_font_size,
                            );
                        }

                        prev_r = Some(r);
                        prev_bbox_size = segment_bbox_size;
                    }

                    out.push_str("    </g>\n");
                }

                out.push_str("  </g>\n");
            }

            Ok(())
        }
    }
}
