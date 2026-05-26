use crate::primitive::{canonical_key_to_css_token as key_to_css_token, escape_xml_attr};
use rubrum_render::chart_data::ChartData;
use rubrum_render::core::geometry::{normalize_deg, polar_to_xy};
use rubrum_render::error::ChartRenderError;
use rubrum_render::layout::{LaneSpec, PlacementBoundaryTicksSpec, TickDirection};
use rubrum_render::options::RgbaColor;
use rubrum_render::style::resolve_lane_style;
use rubrum_render::theme::Theme;

use super::emit::{push_hit_ring, push_line};

#[allow(clippy::too_many_arguments)]
fn render_angle_ticks_spanning_boundary(
    out: &mut String,
    cx: f64,
    cy: f64,
    boundary_r: f64,
    boundary_width: f64,
    angles_deg: &[f64],
    direction: TickDirection,
    len_in: f64,
    len_out: f64,
    offset_in: f64,
    offset_out: f64,
    stroke_width: f64,
    stroke: RgbaColor,
) {
    let w2 = (boundary_width.max(0.0)) / 2.0;

    for deg in angles_deg.iter().copied() {
        match direction {
            TickDirection::Inward => {
                let r0 = (boundary_r - w2 - offset_in).max(0.0);
                let r1 = (r0 - len_in).max(0.0);
                let (x0, y0) = polar_to_xy(cx, cy, r0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, r1, deg);
                push_line(out, x0, y0, x1, y1, stroke, stroke_width);
            }
            TickDirection::Outward => {
                let r0 = boundary_r + w2 + offset_out;
                let r1 = r0 + len_out;
                let (x0, y0) = polar_to_xy(cx, cy, r0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, r1, deg);
                push_line(out, x0, y0, x1, y1, stroke, stroke_width);
            }
            TickDirection::Both => {
                let rin0 = (boundary_r - w2 - offset_in).max(0.0);
                let rin1 = (rin0 - len_in).max(0.0);
                let rout0 = boundary_r + w2 + offset_out;
                let rout1 = rout0 + len_out;

                let (x0, y0) = polar_to_xy(cx, cy, rin0, deg);

                let (x1, y1) = polar_to_xy(cx, cy, rin1, deg);
                push_line(out, x0, y0, x1, y1, stroke, stroke_width);

                let (x0, y0) = polar_to_xy(cx, cy, rout0, deg);
                let (x1, y1) = polar_to_xy(cx, cy, rout1, deg);
                push_line(out, x0, y0, x1, y1, stroke, stroke_width);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_placement_boundary_ticks(
    out: &mut String,
    theme: &Theme,
    lane: &LaneSpec,
    data: &ChartData,
    cx: f64,
    cy: f64,
    boundary_r: f64,
    outer_shared_boundary_width: f64,
    rotation_deg: f64,
    ticks_spec: &PlacementBoundaryTicksSpec,
) -> Result<(), ChartRenderError> {
    let options = &theme.cairo;

    let Some(dataset) = lane.dataset.as_ref() else {
        return Ok(());
    };

    let bodies = data
        .dataset_bodies(dataset)
        .ok_or_else(|| ChartRenderError::InvalidSpec(format!("Unknown dataset '{dataset}'")))?;

    let endpoint_filter = lane.endpoint_filter.as_ref().map(|f| f.compile());

    let mut angles_deg: Vec<f64> = Vec::new();
    for &pm in bodies.iter() {
        if let Some(filter) = endpoint_filter.as_ref()
            && !filter.endpoint_allowed(pm.occupant())
        {
            continue;
        }

        let Some(sign_degree) = pm.coordinate().sign_degree() else {
            continue;
        };
        angles_deg.push(normalize_deg(sign_degree.degrees + rotation_deg));
    }

    let style = resolve_lane_style(theme, lane);
    let dataset_color = theme.dataset_colors.get(dataset).copied();

    let stroke = ticks_spec
        .stroke
        .or(style.stroke)
        .or(dataset_color)
        .unwrap_or(theme.effective_ticks_color());

    let direction = ticks_spec.direction.unwrap_or(TickDirection::Both);

    let tick_len_in = ticks_spec.length_in.unwrap_or(6.0);
    let tick_len_out = ticks_spec.length_out.unwrap_or(4.0);

    let tick_offset_in = ticks_spec.offset_in.unwrap_or(0.0);
    let tick_offset_out = ticks_spec.offset_out.unwrap_or(0.0);

    let tick_width = ticks_spec
        .width
        .unwrap_or((options.stroke_width * 0.6).max(1.0));

    render_angle_ticks_spanning_boundary(
        out,
        cx,
        cy,
        boundary_r,
        outer_shared_boundary_width,
        angles_deg.as_slice(),
        direction,
        tick_len_in,
        tick_len_out,
        tick_offset_in,
        tick_offset_out,
        tick_width,
        stroke,
    );

    // Stable hit target for selecting this dataset-driven tick ring.
    {
        let w2 = (outer_shared_boundary_width.max(0.0)) / 2.0;

        let (r_min, r_max) = match direction {
            TickDirection::Inward => {
                let r_outer = (boundary_r - w2 - tick_offset_in).max(0.0);
                let r_inner = (r_outer - tick_len_in).max(0.0);
                (r_inner, r_outer)
            }
            TickDirection::Outward => {
                let r_inner = boundary_r + w2 + tick_offset_out;
                let r_outer = r_inner + tick_len_out;
                (r_inner.max(0.0), r_outer.max(0.0))
            }
            TickDirection::Both => {
                let r_outer_in = (boundary_r - w2 - tick_offset_in).max(0.0);
                let r_inner_in = (r_outer_in - tick_len_in).max(0.0);
                let r_inner_out = (boundary_r + w2 + tick_offset_out).max(0.0);
                let r_outer_out = (r_inner_out + tick_len_out).max(0.0);
                (r_inner_in.min(r_inner_out), r_outer_in.max(r_outer_out))
            }
        };

        let hit_r = ((r_min + r_max) / 2.0).max(0.0);
        let hit_w = (r_max - r_min).abs().max(14.0);

        let lane_id = lane.id.as_deref().unwrap_or("");
        let lane_id_attr = escape_xml_attr(lane_id);
        let lane_token = if !lane_id.is_empty() {
            key_to_css_token(lane_id)
        } else {
            "idx".to_owned()
        };

        let dataset_attr = escape_xml_attr(dataset.as_str());

        let mut extra = format!(
            "data-rb-structure=\"placement-boundary-ticks\" data-rb-dataset=\"{dataset_attr}\" data-rb-boundary-r=\"{:.3}\"",
            boundary_r
        );
        if !lane_id.is_empty() {
            extra.push_str(&format!(" data-rb-lane-id=\"{lane_id_attr}\""));
        }

        let class =
            format!("rb-placement-boundary-ticks-hit rb-placement-boundary-ticks-hit-{lane_token}");
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

    Ok(())
}
