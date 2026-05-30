use std::collections::HashMap;

use rubrum::AspectRules;
use rubrum_render::aspects::{aspect_kind_group, resolve_aspect_stroke_style};
use rubrum_render::chart_data::ChartData;
use rubrum_render::core::geometry::{normalize_deg, polar_to_xy};
use rubrum_render::core::lane_radii;
use rubrum_render::error::ChartRenderError;
use rubrum_render::layout::{GlyphLaneMode, Layout};
use rubrum_render::options::RgbaColor;
use rubrum_render::theme::Theme;

use crate::primitive::{canonical_key_to_css_token as key_to_css_token, escape_xml_attr, rgba_css};

/// Render aspects for a layout lane configured as `GlyphLaneMode::CrossAspects` **or**
/// `GlyphLaneMode::Aspects` with `other_dataset` set.
///
/// This is implemented in-crate (rather than in `rubrum_svg_helpers`) so we can:
/// - render cross-dataset lines (dataset A ↔ dataset B)
/// - qualify aspect endpoint ids with the dataset id (e.g. `transit:sun` vs `natal:sun`) to avoid collisions
/// - choose endpoint radii based on the actual placement rings so lines span across bands
#[allow(clippy::too_many_arguments)]
pub fn render_cross_dataset_aspects_svg_group(
    theme: &Theme,
    layout: &Layout,
    aspect_rules: Option<&AspectRules>,
    data: &ChartData,
    cx: f64,
    cy: f64,
    rotation_deg: f64,
    base_r_outer: f64,
    band_thicknesses_px: &[f64],
    default_text_color: RgbaColor,
) -> Result<Option<String>, ChartRenderError> {
    if !theme.aspects.enabled {
        return Ok(None);
    }

    // Find the first aspects lane that is configured for cross-dataset computation.
    let mut aspects_lane_dataset: Option<String> = None;
    let mut other_dataset: Option<String> = None;

    for band in &layout.bands {
        for lane in &band.lanes {
            let Some(glyphs) = lane.glyphs.as_ref() else {
                continue;
            };
            match glyphs.mode {
                GlyphLaneMode::CrossAspects => {
                    aspects_lane_dataset = lane.dataset.clone();
                    other_dataset = glyphs.other_dataset.clone();
                    break;
                }
                GlyphLaneMode::Aspects => {
                    if glyphs.other_dataset.is_some() {
                        aspects_lane_dataset = lane.dataset.clone();
                        other_dataset = glyphs.other_dataset.clone();
                        break;
                    }
                }
                _ => {}
            }
        }
        if other_dataset.is_some() {
            break;
        }
    }

    let Some(dataset_a_id) = aspects_lane_dataset.or_else(|| Some("natal".to_owned())) else {
        return Ok(None);
    };
    let Some(dataset_b_id) = other_dataset else {
        return Ok(None);
    };

    let Some(dataset_a) = data.datasets.iter().find(|d| d.id == dataset_a_id) else {
        return Ok(None);
    };
    let Some(dataset_b) = data.datasets.iter().find(|d| d.id == dataset_b_id) else {
        return Ok(None);
    };

    // Choose endpoint radii based on the placement rings for each dataset.
    //
    // For the transit chart layout we want aspect chords to run **tick-to-tick** on the shared
    // boundary between the sign band and the natal placements band.
    //
    // That means both datasets should terminate on the same endpoint radius (not on either
    // dataset's glyph radius).
    let shared_r = shared_boundary_radius_for_transit(layout, base_r_outer, band_thicknesses_px)
        .unwrap_or(base_r_outer * 0.40);

    let r_a = shared_r;
    let r_b = shared_r;

    // Endpoint degree lookup keyed by qualified ids.
    let mut endpoints_a: HashMap<String, f64> = HashMap::new();
    for pm in &dataset_a.bodies {
        let Some(lon) = pm.placement.coordinate.sign_degree().map(|sd| sd.degrees) else {
            continue;
        };

        let id = qualified_endpoint_id(&dataset_a_id, &pm.placement.occupant);
        endpoints_a.insert(id, lon);
    }

    let mut endpoints_b: HashMap<String, f64> = HashMap::new();
    for pm in &dataset_b.bodies {
        let Some(lon) = pm.placement.coordinate.sign_degree().map(|sd| sd.degrees) else {
            continue;
        };

        let id = qualified_endpoint_id(&dataset_b_id, &pm.placement.occupant);
        endpoints_b.insert(id, lon);
    }

    // Compute dataset-scoped cross aspects so the edge endpoint ids are stable and match the
    // qualified ids we store in `endpoints_a` / `endpoints_b`.
    let rules_owned;
    let rules = match aspect_rules {
        Some(r) => r,
        None => {
            rules_owned = AspectRules::default();
            &rules_owned
        }
    };

    let edges = rubrum::aspect::compute::compute_aspects_cross_datasets(
        dataset_a_id.as_str(),
        &dataset_a.bodies,
        dataset_b_id.as_str(),
        &dataset_b.bodies,
        rules,
    );

    let mut out = String::new();
    out.push_str("  <g id=\"rb-aspects\">\n");

    for edge in edges {
        // NOTE: `AspectEdge` is undirected and canonicalizes endpoint ordering (a <= b).
        // That means we cannot assume `edge.a` belongs to dataset A and `edge.b` belongs to
        // dataset B.
        let qid_0 = edge.a.0;
        let qid_1 = edge.b.0;

        // Resolve each endpoint against whichever dataset map contains it.
        let a_res = endpoints_a
            .get(qid_0.as_str())
            .copied()
            .map(|lon| (lon, r_a))
            .or_else(|| {
                endpoints_b
                    .get(qid_0.as_str())
                    .copied()
                    .map(|lon| (lon, r_b))
            });

        let b_res = endpoints_a
            .get(qid_1.as_str())
            .copied()
            .map(|lon| (lon, r_a))
            .or_else(|| {
                endpoints_b
                    .get(qid_1.as_str())
                    .copied()
                    .map(|lon| (lon, r_b))
            });

        let Some((lon_0, r_0)) = a_res else {
            continue;
        };
        let Some((lon_1, r_1)) = b_res else {
            continue;
        };

        let a_deg = normalize_deg(lon_0 + rotation_deg);
        let b_deg = normalize_deg(lon_1 + rotation_deg);

        let (x1, y1) = polar_to_xy(cx, cy, r_0, a_deg);
        let (x2, y2) = polar_to_xy(cx, cy, r_1, b_deg);

        let kind = edge.kind;
        let kind_key = kind.canonical_key();
        let kind_token = key_to_css_token(kind_key);
        let kind_group = aspect_kind_group(&kind);
        let kind_group_key = match kind_group {
            rubrum_render::aspects::AspectKindGroup::Hard => "hard",
            rubrum_render::aspects::AspectKindGroup::Soft => "soft",
            rubrum_render::aspects::AspectKindGroup::Neutral => "neutral",
            rubrum_render::aspects::AspectKindGroup::Minor => "minor",
        };
        let stroke_style = resolve_aspect_stroke_style(
            &theme.aspects,
            &kind,
            default_text_color,
            theme.cairo.stroke_width,
        );
        let stroke_css = rgba_css(stroke_style.color);
        let stroke_width = stroke_style.width;
        let dash_attr = stroke_style
            .dash
            .as_deref()
            .and_then(rubrum_render::svg::fmt_stroke_dasharray_attr)
            .unwrap_or_default();
        let linecap_attr = stroke_style
            .linecap
            .map(|linecap| {
                format!(
                    " stroke-linecap=\"{}\"",
                    rubrum_render::svg::fmt_stroke_linecap_attr(linecap)
                )
            })
            .unwrap_or_default();

        // Stable data attributes for downstream selection.
        let a_attr = escape_xml_attr(qid_0.as_str());
        let b_attr = escape_xml_attr(qid_1.as_str());

        out.push_str(&format!(
            "    <line class=\"rb-aspect rb-aspect-{kind_token} rb-aspect-group-{kind_group_key}\" x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\" stroke=\"{stroke_css}\" stroke-width=\"{stroke_width:.3}\"{dash_attr}{linecap_attr} data-rb-aspect-kind=\"{kind_key}\" data-rb-aspect-group=\"{kind_group_key}\" data-rb-endpoint-a=\"{a_attr}\" data-rb-endpoint-b=\"{b_attr}\" />\n"
        ));
    }

    out.push_str("  </g>\n");
    Ok(Some(out))
}

fn qualified_endpoint_id(dataset_id: &str, occupant: &rubrum::Occupant) -> String {
    // Match the stable endpoint ids used by `rubrum::AspectEndpointId`.
    format!("{}:{}", dataset_id, occupant.canonical_key())
}

#[allow(dead_code)]

fn placement_ring_radius_for_dataset(
    layout: &Layout,
    base_r_outer: f64,
    band_thicknesses_px: &[f64],
    dataset_id: &str,
) -> Option<f64> {
    let mut r_outer = base_r_outer;

    for (band_idx, band) in layout.bands.iter().enumerate() {
        let band_thickness_px = *band_thicknesses_px.get(band_idx)?;
        let r_inner = (r_outer - band_thickness_px).max(0.0);

        let lane_count = band.lanes.len();
        let lane_thickness = if lane_count > 0 {
            band_thickness_px / (lane_count as f64)
        } else {
            0.0
        };

        for (lane_idx, lane) in band.lanes.iter().enumerate() {
            let Some(glyphs) = lane.glyphs.as_ref() else {
                continue;
            };

            if glyphs.mode != GlyphLaneMode::Bodies {
                continue;
            }

            if lane.dataset.as_deref() != Some(dataset_id) {
                continue;
            }

            let (lane_r_inner, lane_r_outer) = lane_radii(r_outer, lane_thickness, lane_idx);
            let radial_bias = glyphs.radial_bias.unwrap_or(0.5).clamp(0.0, 1.0);
            let glyph_r = lane_r_inner + (lane_r_outer - lane_r_inner) * radial_bias;
            return Some(glyph_r);
        }

        r_outer = r_inner;
    }

    None
}

#[allow(dead_code)]
fn outer_boundary_radius_for_bodies_dataset(
    layout: &Layout,
    base_r_outer: f64,
    band_thicknesses_px: &[f64],
    dataset_id: &str,
) -> Option<f64> {
    let mut r_outer = base_r_outer;

    for (band_idx, band) in layout.bands.iter().enumerate() {
        let band_thickness_px = *band_thicknesses_px.get(band_idx)?;
        let r_inner = (r_outer - band_thickness_px).max(0.0);

        let any_matching_lane = band.lanes.iter().any(|lane| {
            lane.dataset.as_deref() == Some(dataset_id)
                && lane
                    .glyphs
                    .as_ref()
                    .is_some_and(|g| g.mode == GlyphLaneMode::Bodies)
        });

        if any_matching_lane {
            return Some(r_outer);
        }

        r_outer = r_inner;
    }

    None
}

fn inner_boundary_radius_for_bodies_dataset(
    layout: &Layout,
    base_r_outer: f64,
    band_thicknesses_px: &[f64],
    dataset_id: &str,
) -> Option<f64> {
    let mut r_outer = base_r_outer;

    for (band_idx, band) in layout.bands.iter().enumerate() {
        let band_thickness_px = *band_thicknesses_px.get(band_idx)?;
        let r_inner = (r_outer - band_thickness_px).max(0.0);

        let any_matching_lane = band.lanes.iter().any(|lane| {
            lane.dataset.as_deref() == Some(dataset_id)
                && lane
                    .glyphs
                    .as_ref()
                    .is_some_and(|g| g.mode == GlyphLaneMode::Bodies)
        });

        if any_matching_lane {
            return Some(r_inner);
        }

        r_outer = r_inner;
    }

    None
}

fn shared_boundary_radius_for_transit(
    layout: &Layout,
    base_r_outer: f64,
    band_thicknesses_px: &[f64],
) -> Option<f64> {
    // For the transit layout, we want aspect chords to terminate on the *house-dividing circle*
    // that sits one boundary inward from the natal placements band's outer edge.
    //
    // Concretely: use the natal placements band's **inner** boundary, which is the same radius as
    // the house_numbers band's **outer** boundary.
    inner_boundary_radius_for_bodies_dataset(layout, base_r_outer, band_thicknesses_px, "natal")
}
