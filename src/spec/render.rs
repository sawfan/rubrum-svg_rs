use rubrum_render::chart_data::ChartData;
use rubrum_render::core::render_plan::{RenderPlan, plan_chart_spec};
use rubrum_render::core::{lane_radii, resolve_band_thicknesses};
use rubrum_render::error::ChartRenderError;
use rubrum_render::layout::Layout;
use rubrum_render::options::RgbaColor;
use rubrum_render::theme::Theme;
use rubrum_svg_helpers::{AspectsSvgGroupOptions, build_rb_aspects_svg_group};

use super::aspects::render_cross_dataset_aspects_svg_group;

use crate::primitive::rgba_css_var;

use super::band::render_band_structure;
use super::placements::render_lane_glyphs;

fn render_band_glyphs(
    out: &mut String,
    theme: &Theme,
    data: &ChartData,
    band: &rubrum_render::layout::BandSpec,
    cx: f64,
    cy: f64,
    r_outer: f64,
    band_thickness_px: f64,
    rotation_deg: f64,
    default_text_color: RgbaColor,
) -> Result<(), ChartRenderError> {
    let lane_count = band.lanes.len();
    let lane_thickness = if lane_count > 0 {
        band_thickness_px / (lane_count as f64)
    } else {
        0.0
    };

    // Lane glyphs.
    for (i, lane) in band.lanes.iter().enumerate() {
        let (lane_r_inner, lane_r_outer) = lane_radii(r_outer, lane_thickness, i);
        render_lane_glyphs(
            out,
            theme,
            lane,
            data,
            cx,
            cy,
            lane_r_inner,
            lane_r_outer,
            rotation_deg,
            default_text_color,
        )?;
    }

    Ok(())
}

fn render_chart_group_to_svg(
    theme: &Theme,
    layout: &Layout,
    aspect_rules: Option<&rubrum::AspectRules>,
    data: &ChartData,
) -> Result<(RenderPlan, String), ChartRenderError> {
    let plan: RenderPlan = plan_chart_spec(theme, layout, data)?;

    let default_text_color = plan
        .foreground
        .unwrap_or_else(|| theme.effective_text_color());

    let cx = plan.center.x;
    let cy = plan.center.y;
    let rotation_deg = plan.rotation_deg;
    let base_r_outer = plan.base_r_outer;

    let band_thicknesses_px = resolve_band_thicknesses(layout, base_r_outer)?;

    // Precompute per-band radii so we can do multiple passes (structure → aspects → glyphs).
    let mut band_geoms: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(layout.bands.len());
    let mut r_outer = base_r_outer;
    let mut prev_inner_boundary_width = 0.0;

    for (band, band_thickness_px) in layout.bands.iter().zip(band_thicknesses_px.iter().copied()) {
        if band_thickness_px <= 0.0 {
            return Err(ChartRenderError::InvalidSpec(format!(
                "Band '{}' has non-positive thickness",
                band.id
            )));
        }

        let r_inner = (r_outer - band_thickness_px).max(0.0);
        let outer_shared_boundary_width = prev_inner_boundary_width;

        band_geoms.push((
            r_inner,
            r_outer,
            band_thickness_px,
            outer_shared_boundary_width,
        ));

        prev_inner_boundary_width = band
            .boundary
            .as_ref()
            .map(|b| b.width.unwrap_or(theme.cairo.stroke_width))
            .unwrap_or(0.0);

        r_outer = r_inner;
    }

    let mut out = String::new();

    out.push_str("  <g id=\"rb-chart\">\n");

    // Pass 1: structure (fills, boundaries, ticks, houses, signs).
    for (band, (r_inner, r_outer, band_thickness_px, outer_shared_boundary_width)) in
        layout.bands.iter().zip(band_geoms.iter().copied())
    {
        render_band_structure(
            &mut out,
            theme,
            data,
            band,
            cx,
            cy,
            r_inner,
            r_outer,
            band_thickness_px,
            outer_shared_boundary_width,
            rotation_deg,
            default_text_color,
        )?;
    }

    // Pass 2: aspects (under placement glyphs).
    {
        // Prefer cross-dataset aspect rendering when configured.
        if let Some(group) = render_cross_dataset_aspects_svg_group(
            theme,
            layout,
            aspect_rules,
            data,
            cx,
            cy,
            rotation_deg,
            base_r_outer,
            band_thicknesses_px.as_slice(),
            default_text_color,
        )? {
            out.push_str(group.as_str());
        } else {
            let opts = AspectsSvgGroupOptions::for_svg_backend(default_text_color);
            if let Some(group) = build_rb_aspects_svg_group(
                theme,
                layout,
                aspect_rules,
                data,
                cx,
                cy,
                rotation_deg,
                base_r_outer,
                band_thicknesses_px.as_slice(),
                opts,
            )? {
                out.push_str(group.as_str());
            }
        }
    }

    // Pass 3: placement glyphs/text.
    for (band, (_r_inner, r_outer, band_thickness_px, _outer_shared_boundary_width)) in
        layout.bands.iter().zip(band_geoms.iter().copied())
    {
        render_band_glyphs(
            &mut out,
            theme,
            data,
            band,
            cx,
            cy,
            r_outer,
            band_thickness_px,
            rotation_deg,
            default_text_color,
        )?;
    }

    out.push_str("  </g>\n");
    Ok((plan, out))
}

fn render_chart_to_svg(
    theme: &Theme,
    layout: &Layout,
    aspect_rules: Option<&rubrum::AspectRules>,
    data: &ChartData,
) -> Result<String, ChartRenderError> {
    let (plan, group) = render_chart_group_to_svg(theme, layout, aspect_rules, data)?;

    let w = plan.canvas.width;
    let h = plan.canvas.height;

    let bg = theme.effective_cairo_background();
    let bg_css = rgba_css_var("--rb-chart-canvas-bg", bg);

    let cx = plan.center.x;
    let cy = plan.center.y;
    let rotation_deg = plan.rotation_deg;

    let mut out = String::new();
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" overflow=\"visible\" data-rb-cx=\"{cx}\" data-rb-cy=\"{cy}\" data-rb-rotation-deg=\"{rotation_deg}\">\n"
    ));

    // Background.
    if bg.a > 0.0 {
        out.push_str(&format!(
            "  <rect width=\"100%\" height=\"100%\" fill=\"{bg_css}\" />\n"
        ));
    }

    out.push_str(group.as_str());
    out.push_str("</svg>\n");

    Ok(out)
}

/// Render a chart to an SVG **string** using the spec inputs.
///
/// This is the pure-SVG (no Cairo) renderer entrypoint.
pub fn chart_to_svg_string_spec(
    theme: &Theme,
    layout: &Layout,
    aspect_rules: Option<&rubrum::AspectRules>,
    data: &ChartData,
) -> Result<String, ChartRenderError> {
    render_chart_to_svg(theme, layout, aspect_rules, data)
}
