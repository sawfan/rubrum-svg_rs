use crate::primitive::annulus_path;
use rubrum_render::chart_data::ChartData;
use rubrum_render::core::geometry::{normalize_deg, polar_to_xy};
use rubrum_render::error::ChartRenderError;
use rubrum_render::layout::{BandSpec, TickAnchor, TickDirection};
use rubrum_render::options::RgbaColor;
use rubrum_render::style::resolve_lane_style;

use crate::primitive::{
    canonical_key_to_css_token, circle_extra, escape_xml_attr, hit_ring, line_extra, path_d,
    rgba_css, rgba_css_var,
};
use rubrum_render::theme::Theme;

use super::emit::push_hit_ring;

use super::houses::render_houses;
use super::placement_boundary_ticks::render_placement_boundary_ticks;
use super::signs::render_signs;
use super::ticks::{tick_anchor_token, tick_direction_token};
use crate::primitive::push_svg_node;

#[allow(clippy::too_many_arguments)]
fn render_ticks_1deg(
    out: &mut String,
    cx: f64,
    cy: f64,
    boundary_r: f64,
    boundary_width: f64,
    rotation_deg: f64,
    direction: TickDirection,
    stroke: RgbaColor,
    stroke_width: f64,
    major_len_in: f64,
    major_len_out: f64,
) {
    let w2 = (boundary_width.max(0.0)) / 2.0;

    for d in 0..360 {
        let deg = normalize_deg(d as f64 + rotation_deg);

        let (len_in, len_out, w_mul) = if d % 10 == 0 {
            (major_len_in, major_len_out, 1.2)
        } else if d % 5 == 0 {
            (major_len_in * 0.65, major_len_out * 0.65, 1.0)
        } else {
            (major_len_in * 0.35, major_len_out * 0.35, 0.8)
        };

        let width = (stroke_width.max(0.5)) * w_mul;

        match direction {
            TickDirection::Inward => {
                let r0 = (boundary_r - w2).max(0.0);
                let r1 = (r0 - len_in).max(0.0);
                let (x0, y0) = polar_to_xy(cx, cy, r0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, r1, deg);
                if let Some(node) = line_extra(x0, y0, x1, y1, stroke, width, None, None) {
                    push_svg_node(out, "  ", node);
                }
            }
            TickDirection::Outward => {
                let r0 = boundary_r + w2;
                let r1 = r0 + len_out;
                let (x0, y0) = polar_to_xy(cx, cy, r0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, r1, deg);
                if let Some(node) = line_extra(x0, y0, x1, y1, stroke, width, None, None) {
                    push_svg_node(out, "  ", node);
                }
            }
            TickDirection::Both => {
                let rin0 = (boundary_r - w2).max(0.0);
                let rin1 = (rin0 - len_in).max(0.0);
                let rout0 = boundary_r + w2;
                let rout1 = rout0 + len_out;

                let (x0, y0) = polar_to_xy(cx, cy, rin0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, rin1, deg);
                if let Some(node) = line_extra(x0, y0, x1, y1, stroke, width, None, None) {
                    push_svg_node(out, "  ", node);
                }

                let (x0, y0) = polar_to_xy(cx, cy, rout0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, rout1, deg);
                if let Some(node) = line_extra(x0, y0, x1, y1, stroke, width, None, None) {
                    push_svg_node(out, "  ", node);
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_band_structure(
    out: &mut String,
    theme: &Theme,
    data: &ChartData,
    band: &BandSpec,
    cx: f64,
    cy: f64,
    r_inner: f64,
    r_outer: f64,
    band_thickness_px: f64,
    outer_shared_boundary_width: f64,
    rotation_deg: f64,
    zodiac_rotation_deg: f64,
    default_text_color: RgbaColor,
) -> Result<(), ChartRenderError> {
    let options = &theme.cairo;

    let band_id_attr = escape_xml_attr(band.id.as_str());
    let band_id_token = canonical_key_to_css_token(band.id.as_str());

    // Band fill.
    if let Some(fill) = band.fill {
        let d = annulus_path(cx, cy, r_inner, r_outer);
        if !d.is_empty() {
            let fill = rgba_css_var("--rb-chart-band-fill", fill);

            let class = format!("rb-band-fill rb-band-fill-{band_id_token}");
            let extra = format!(
                "data-rb-structure=\"band-fill\" data-rb-band=\"{band_id_attr}\" fill=\"{fill}\" fill-rule=\"evenodd\" pointer-events=\"all\""
            );

            if let Some(node) = path_d(d.as_str(), Some(class.as_str()), Some(extra.as_str())) {
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }
        }
    }

    // Always emit a stable hit target for the band so the inspector can select it even when
    // the fill is omitted or fully transparent.
    {
        let hit_r = (r_inner + r_outer) / 2.0;
        let hit_w = band_thickness_px.max(10.0);
        let extra = format!(
            "data-rb-structure=\"band\" data-rb-band=\"{band_id_attr}\" data-rb-r-inner=\"{r_inner}\" data-rb-r-outer=\"{r_outer}\""
        );

        let class = format!("rb-band-hit rb-band-hit-{band_id_token}");
        if let Some(node) = hit_ring(cx, cy, hit_r, hit_w, class.as_str(), Some(extra.as_str())) {
            out.push_str("  ");
            out.push_str(node.to_string().as_str());
            out.push('\n');
        }
    }

    let lane_count = band.lanes.len();
    let lane_thickness = if lane_count > 0 {
        band_thickness_px / (lane_count as f64)
    } else {
        0.0
    };

    // Lane fills (template + overrides).
    for (i, lane) in band.lanes.iter().enumerate() {
        let (lane_r_inner, lane_r_outer) =
            rubrum_render::core::lane_radii(r_outer, lane_thickness, i);

        let lane_id = lane.id.as_deref().unwrap_or("");
        let lane_id_attr = escape_xml_attr(lane_id);
        let lane_token = if !lane_id.is_empty() {
            canonical_key_to_css_token(lane_id)
        } else {
            format!("idx-{i}")
        };

        let style = resolve_lane_style(theme, lane);
        if let Some(fill) = style.fill {
            let d = annulus_path(cx, cy, lane_r_inner, lane_r_outer);
            if !d.is_empty() {
                let fill = rgba_css(fill);

                let mut extra = format!(
                    "data-rb-structure=\"lane-fill\" data-rb-band=\"{band_id_attr}\" data-rb-lane-index=\"{i}\""
                );
                if !lane_id.is_empty() {
                    extra.push_str(&format!(" data-rb-lane-id=\"{lane_id_attr}\""));
                }

                let class = format!("rb-lane-fill rb-lane-fill-{band_id_token}-{lane_token}");
                let extra =
                    format!("{extra} fill=\"{fill}\" fill-rule=\"evenodd\" pointer-events=\"all\"");

                if let Some(node) = path_d(d.as_str(), Some(class.as_str()), Some(extra.as_str())) {
                    out.push_str("  ");
                    out.push_str(node.to_string().as_str());
                    out.push('\n');
                }
            }
        }

        // Always emit a stable hit target for each lane. This enables selecting a lane even when
        // its fill is omitted or fully transparent.
        {
            let hit_r = (lane_r_inner + lane_r_outer) / 2.0;
            let hit_w = lane_thickness.max(8.0);

            let mut extra = format!(
                "data-rb-structure=\"lane\" data-rb-band=\"{band_id_attr}\" data-rb-lane-index=\"{i}\" data-rb-r-inner=\"{lane_r_inner}\" data-rb-r-outer=\"{lane_r_outer}\""
            );
            if !lane_id.is_empty() {
                extra.push_str(&format!(" data-rb-lane-id=\"{lane_id_attr}\""));
            }

            let class = format!("rb-lane-hit rb-lane-hit-{band_id_token}-{lane_token}");
            if let Some(node) = hit_ring(cx, cy, hit_r, hit_w, class.as_str(), Some(extra.as_str()))
            {
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }
        }
    }

    // Band boundaries.
    if let Some(boundary) = band.boundary.as_ref() {
        let width = boundary.width.unwrap_or(options.stroke_width).max(0.5);
        let color = boundary.color.unwrap_or(theme.effective_structure_color());

        let hit_w = (width * 6.0).max(14.0);

        // Outer boundary.
        {
            let extra = format!(
                "data-rb-structure=\"band-boundary\" data-rb-band=\"{band_id_attr}\" data-rb-boundary=\"outer\" data-rb-r=\"{:.3}\"",
                r_outer
            );
            let class =
                format!("rb-band-boundary rb-band-boundary-{band_id_token} rb-band-boundary-outer");
            if let Some(node) = circle_extra(
                cx,
                cy,
                r_outer,
                color,
                width,
                Some(class.as_str()),
                Some(extra.as_str()),
            ) {
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }

            let hit_class = format!(
                "rb-band-boundary-hit rb-band-boundary-hit-{band_id_token} rb-band-boundary-hit-outer"
            );
            if let Some(node) = hit_ring(
                cx,
                cy,
                r_outer,
                hit_w,
                hit_class.as_str(),
                Some(extra.as_str()),
            ) {
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }
        }

        // Inner boundary.
        {
            let extra = format!(
                "data-rb-structure=\"band-boundary\" data-rb-band=\"{band_id_attr}\" data-rb-boundary=\"inner\" data-rb-r=\"{:.3}\"",
                r_inner
            );
            let class =
                format!("rb-band-boundary rb-band-boundary-{band_id_token} rb-band-boundary-inner");
            if let Some(node) = circle_extra(
                cx,
                cy,
                r_inner,
                color,
                width,
                Some(class.as_str()),
                Some(extra.as_str()),
            ) {
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }

            let hit_class = format!(
                "rb-band-boundary-hit rb-band-boundary-hit-{band_id_token} rb-band-boundary-hit-inner"
            );
            if let Some(node) = hit_ring(
                cx,
                cy,
                r_inner,
                hit_w,
                hit_class.as_str(),
                Some(extra.as_str()),
            ) {
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }
        }

        // Lane separators.
        if lane_count > 1 {
            for i in 1..lane_count {
                let sep_r = r_outer - lane_thickness * (i as f64);

                let extra = format!(
                    "data-rb-structure=\"lane-separator\" data-rb-band=\"{band_id_attr}\" data-rb-lane-separator-index=\"{i}\" data-rb-r=\"{:.3}\"",
                    sep_r
                );
                let class = format!(
                    "rb-lane-separator rb-lane-separator-{band_id_token} rb-lane-separator-idx-{i}"
                );

                if let Some(node) = circle_extra(
                    cx,
                    cy,
                    sep_r,
                    color,
                    width,
                    Some(class.as_str()),
                    Some(extra.as_str()),
                ) {
                    out.push_str("  ");
                    out.push_str(node.to_string().as_str());
                    out.push('\n');
                }

                let hit_class = format!(
                    "rb-lane-separator-hit rb-lane-separator-hit-{band_id_token} rb-lane-separator-hit-idx-{i}"
                );
                if let Some(node) = hit_ring(
                    cx,
                    cy,
                    sep_r,
                    hit_w,
                    hit_class.as_str(),
                    Some(extra.as_str()),
                ) {
                    out.push_str("  ");
                    out.push_str(node.to_string().as_str());
                    out.push('\n');
                }
            }
        }
    }

    // Houses.
    render_houses(
        out,
        theme,
        band,
        data,
        cx,
        cy,
        r_inner,
        r_outer,
        rotation_deg,
        default_text_color,
    )?;

    // Signs.
    render_signs(
        out,
        theme,
        band,
        cx,
        cy,
        r_inner,
        r_outer,
        zodiac_rotation_deg,
        default_text_color,
    );

    // 1° ticks.
    for (tick_slot, ticks) in [
        ("ticks_inner", band.ticks_inner.as_ref()),
        ("ticks_outer", band.ticks_outer.as_ref()),
    ] {
        let Some(ticks) = ticks else {
            continue;
        };
        if !ticks.enabled {
            continue;
        }

        let direction = ticks.direction.unwrap_or(TickDirection::Outward);

        let anchor = ticks
            .anchor
            .or(match direction {
                TickDirection::Inward => Some(TickAnchor::Inner),
                TickDirection::Outward => Some(TickAnchor::Outer),
                TickDirection::Both => Some(TickAnchor::Outer),
            })
            .unwrap_or(TickAnchor::Outer);

        let boundary_r = match anchor {
            TickAnchor::Inner => r_inner,
            TickAnchor::Outer => r_outer,
        };

        let boundary_width = band
            .boundary
            .as_ref()
            .map(|b| b.width.unwrap_or(options.stroke_width))
            .unwrap_or(0.0);

        let stroke = ticks.stroke.unwrap_or(theme.effective_ticks_color());
        let major_len_in = ticks.length_in.unwrap_or(10.0);
        let major_len_out = ticks.length_out.unwrap_or(10.0);

        render_ticks_1deg(
            out,
            cx,
            cy,
            boundary_r,
            boundary_width,
            zodiac_rotation_deg,
            direction,
            stroke,
            options.stroke_width,
            major_len_in,
            major_len_out,
        );

        // Stable hit target for selecting the whole tick system (instead of individual 1° lines).
        // This dramatically improves usability in the browser inspector.
        {
            let (hit_r, hit_w) = match direction {
                TickDirection::Outward => {
                    let r0 = boundary_r + (boundary_width / 2.0);
                    (
                        r0 + (major_len_out / 2.0),
                        (major_len_out + boundary_width).max(14.0),
                    )
                }
                TickDirection::Inward => {
                    let r0 = (boundary_r - (boundary_width / 2.0)).max(0.0);
                    (
                        (r0 - (major_len_in / 2.0)).max(0.0),
                        (major_len_in + boundary_width).max(14.0),
                    )
                }
                TickDirection::Both => (
                    boundary_r.max(0.0),
                    (major_len_in + major_len_out + boundary_width).max(16.0),
                ),
            };

            let extra = format!(
                "data-rb-structure=\"ticks-1deg\" data-rb-band=\"{band_id_attr}\" data-rb-ticks-slot=\"{tick_slot}\" data-rb-anchor=\"{}\" data-rb-direction=\"{}\" data-rb-boundary-r=\"{:.3}\"",
                tick_anchor_token(anchor),
                tick_direction_token(direction),
                boundary_r
            );

            let class = format!(
                "rb-ticks-hit rb-ticks-hit-{band_id_token} rb-ticks-hit-{band_id_token}-{tick_slot}"
            );
            push_hit_ring(
                out,
                cx,
                cy,
                hit_r,
                hit_w,
                class.as_str(),
                Some(extra.as_str()),
            );
        }
    }

    // Dataset-driven placement boundary ticks (lane-driven).
    for lane in band.lanes.iter() {
        let Some(glyphs) = lane.glyphs.as_ref() else {
            continue;
        };

        let Some(spec) = glyphs.placement_boundary_ticks.as_ref() else {
            continue;
        };

        if !spec.enabled {
            continue;
        }

        let anchor = spec.anchor.unwrap_or(TickAnchor::Outer);
        let boundary_r = match anchor {
            TickAnchor::Inner => r_inner,
            TickAnchor::Outer => r_outer,
        };

        render_placement_boundary_ticks(
            out,
            theme,
            lane,
            data,
            cx,
            cy,
            boundary_r,
            outer_shared_boundary_width,
            zodiac_rotation_deg,
            spec,
        )?;
    }

    Ok(())
}
