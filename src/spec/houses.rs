use rubrum::{Angle, House, Occupant};
use rubrum_render::chart_data::ChartData;
use rubrum_render::core::geometry::{normalize_deg, polar_to_xy};
use rubrum_render::error::ChartRenderError;
use rubrum_render::layout::{BandSpec, HouseNumberCorner, HouseNumberPlacementMode};
use rubrum_render::metadata::svg_data::{
    DATA_RB_AXIS, DATA_RB_BAND, DATA_RB_DATASET, DATA_RB_DEG, DATA_RB_HOUSE, DATA_RB_HOUSE_SET,
    DATA_RB_STRUCTURE,
};
use rubrum_render::options::RgbaColor;
use rubrum_render::theme::Theme;

use super::emit::{push_hit_line, push_line_extra, push_text_extra};

#[allow(clippy::too_many_arguments)]
pub fn render_houses(
    out: &mut String,
    theme: &Theme,
    band: &BandSpec,
    data: &ChartData,
    cx: f64,
    cy: f64,
    r_inner: f64,
    r_outer: f64,
    rotation_deg: f64,
    default_text_color: RgbaColor,
) -> Result<(), ChartRenderError> {
    let options = &theme.cairo;

    let Some(houses) = band.houses.as_ref() else {
        return Ok(());
    };

    if !houses.enabled {
        return Ok(());
    }

    let legacy_spoke_stroke = houses
        .spoke_stroke
        .or_else(|| band.boundary.as_ref().and_then(|b| b.color))
        .unwrap_or(theme.effective_structure_color());

    let legacy_spoke_width = houses.spoke_width.unwrap_or(options.stroke_width);

    let spoke_stroke_spec = houses.spoke.as_ref();
    let spoke_stroke = spoke_stroke_spec
        .and_then(|s| s.color)
        .unwrap_or(legacy_spoke_stroke);
    let spoke_width = spoke_stroke_spec
        .and_then(|s| s.width)
        .unwrap_or(legacy_spoke_width)
        .max(0.5);

    let number_color = houses.number_color.unwrap_or(default_text_color);
    let number_font_size = houses
        .number_font_size
        .unwrap_or((options.label_font_size * 0.8).max(10.0));

    let band_id_attr = crate::primitive::escape_xml_attr(band.id.as_str());

    let house_set_id = houses.house_set.as_deref().unwrap_or("natal");
    let house_set_attr = crate::primitive::escape_xml_attr(house_set_id);
    // For house divisions, the resolved dataset id matches the configured house set id.
    // This enables downstream tooling to select house grids by dataset in multi-wheel charts.
    let dataset_attr = crate::primitive::escape_xml_attr(house_set_id);

    let cusps = data.house_set_cusps(house_set_id).ok_or_else(|| {
        ChartRenderError::InvalidSpec(format!("Unknown house_set '{house_set_id}'"))
    })?;

    if cusps.len() < 12 {
        return Ok(());
    }

    let spoke_inner_r = if houses.spoke_to_center { 0.0 } else { r_inner };

    // Spokes + optional numbers.
    let houses_order = House::default_order();
    for (idx, house) in houses_order.iter().copied().enumerate() {
        let Some(cusp_deg) = cusps
            .iter()
            .find(|c| c.house == house)
            .map(|c| c.sign_degree.degrees)
        else {
            continue;
        };

        let lon_deg = normalize_deg(cusp_deg + rotation_deg);
        let (x1, y1) = polar_to_xy(cx, cy, spoke_inner_r, lon_deg);
        let (x2, y2) = polar_to_xy(cx, cy, r_outer, lon_deg);

        let house_num = house.to_1_based_i32();
        let extra = format!(
            "{DATA_RB_STRUCTURE}=\"house-spoke\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_DATASET}=\"{dataset_attr}\" {DATA_RB_HOUSE_SET}=\"{house_set_attr}\" {DATA_RB_HOUSE}=\"{house_num}\" {DATA_RB_DEG}=\"{:.3}\"",
            lon_deg
        );
        let class = format!("rb-house-spoke rb-house-spoke-house-{house_num}");

        push_line_extra(
            out,
            x1,
            y1,
            x2,
            y2,
            spoke_stroke,
            spoke_width,
            Some(class.as_str()),
            Some(extra.as_str()),
        );

        let hit_w = (spoke_width * 6.0).max(14.0);
        push_hit_line(
            out,
            x1,
            y1,
            x2,
            y2,
            hit_w,
            "rb-house-spoke-hit",
            Some(extra.as_str()),
        );

        if houses.numbers {
            let placement = &houses.number_placement;

            let (label_lon_deg, label_r) = match placement.mode {
                HouseNumberPlacementMode::Midpoint => {
                    let next_house = houses_order[(idx + 1) % houses_order.len()];
                    let next_cusp_deg = cusps
                        .iter()
                        .find(|c| c.house == next_house)
                        .map(|c| c.sign_degree.degrees)
                        .unwrap_or(cusp_deg + 30.0);

                    let delta = normalize_deg(next_cusp_deg - cusp_deg);
                    let mid_abs = cusp_deg + (delta / 2.0);
                    let mid_lon_deg = normalize_deg(mid_abs + rotation_deg);

                    (mid_lon_deg, (r_inner + r_outer) / 2.0)
                }
                HouseNumberPlacementMode::CuspStart => {
                    let lon = normalize_deg(lon_deg + placement.angle_offset_deg);

                    let r = match placement.corner {
                        HouseNumberCorner::Outer => {
                            (r_outer - placement.radial_padding).max(r_inner)
                        }
                        HouseNumberCorner::Inner => r_inner + placement.radial_padding,
                    };

                    (lon, r)
                }
            };

            let (tx, ty) = polar_to_xy(cx, cy, label_r, label_lon_deg);
            let text = house_num.to_string();

            let label_extra = format!(
                "{DATA_RB_STRUCTURE}=\"house-number\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_DATASET}=\"{dataset_attr}\" {DATA_RB_HOUSE_SET}=\"{house_set_attr}\" {DATA_RB_HOUSE}=\"{house_num}\" {DATA_RB_DEG}=\"{:.3}\"",
                label_lon_deg
            );
            let label_class = format!("rb-house-number rb-house-number-house-{house_num}");

            push_text_extra(
                out,
                tx,
                ty,
                text.as_str(),
                number_color,
                options.font_family.as_str(),
                number_font_size,
                Some(label_class.as_str()),
                Some(label_extra.as_str()),
            );
        }
    }

    // Major axes.
    if let Some(axes) = houses.axes.as_ref()
        && axes.enabled
    {
        let axis_inner_r = if axes.to_center { 0.0 } else { r_inner };
        let axis_outer_r = r_outer;

        let axis_stroke = axes
            .stroke
            .as_ref()
            .and_then(|s| s.color)
            .unwrap_or(spoke_stroke);

        let axis_width = axes
            .stroke
            .as_ref()
            .and_then(|s| s.width)
            .unwrap_or(spoke_width * 2.0)
            .max(0.5);

        let legacy_chart = data.to_legacy_chart();

        let asc_deg = cusps
            .iter()
            .find(|c| c.house == House::First)
            .map(|c| c.sign_degree.degrees)
            .or_else(|| {
                legacy_chart
                    .placements_of(Occupant::Angle(Angle::Ascendant))
                    .into_iter()
                    .find_map(|p| p.coordinate.sign_degree())
                    .map(|sd| sd.degrees)
            });

        let mc_deg = cusps
            .iter()
            .find(|c| c.house == House::Tenth)
            .map(|c| c.sign_degree.degrees)
            .or_else(|| {
                legacy_chart
                    .placements_of(Occupant::Angle(Angle::Midheaven))
                    .into_iter()
                    .find_map(|p| p.coordinate.sign_degree())
                    .map(|sd| sd.degrees)
            });

        if axes.asc_desc {
            if let Some(asc) = asc_deg {
                let asc_lon = normalize_deg(asc + rotation_deg);
                let desc_lon = normalize_deg(asc + 180.0 + rotation_deg);

                // ASC axis.
                {
                    let (x1, y1) = polar_to_xy(cx, cy, axis_inner_r, asc_lon);
                    let (x2, y2) = polar_to_xy(cx, cy, axis_outer_r, asc_lon);

                    let extra = format!(
                        "{DATA_RB_STRUCTURE}=\"house-axis\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_DATASET}=\"{dataset_attr}\" {DATA_RB_HOUSE_SET}=\"{house_set_attr}\" {DATA_RB_AXIS}=\"asc\" {DATA_RB_DEG}=\"{:.3}\"",
                        asc_lon
                    );

                    push_line_extra(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        axis_stroke,
                        axis_width,
                        Some("rb-house-axis rb-house-axis-asc"),
                        Some(extra.as_str()),
                    );

                    let hit_w = (axis_width * 6.0).max(14.0);
                    push_hit_line(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        hit_w,
                        "rb-house-axis-hit",
                        Some(extra.as_str()),
                    );
                }

                // DESC axis.
                {
                    let (x1, y1) = polar_to_xy(cx, cy, axis_inner_r, desc_lon);
                    let (x2, y2) = polar_to_xy(cx, cy, axis_outer_r, desc_lon);

                    let extra = format!(
                        "{DATA_RB_STRUCTURE}=\"house-axis\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_DATASET}=\"{dataset_attr}\" {DATA_RB_HOUSE_SET}=\"{house_set_attr}\" {DATA_RB_AXIS}=\"desc\" {DATA_RB_DEG}=\"{:.3}\"",
                        desc_lon
                    );

                    push_line_extra(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        axis_stroke,
                        axis_width,
                        Some("rb-house-axis rb-house-axis-desc"),
                        Some(extra.as_str()),
                    );

                    let hit_w = (axis_width * 6.0).max(14.0);
                    push_hit_line(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        hit_w,
                        "rb-house-axis-hit",
                        Some(extra.as_str()),
                    );
                }
            }
        }

        if axes.mc_ic {
            if let Some(mc) = mc_deg {
                let mc_lon = normalize_deg(mc + rotation_deg);
                let ic_lon = normalize_deg(mc + 180.0 + rotation_deg);

                // MC axis.
                {
                    let (x1, y1) = polar_to_xy(cx, cy, axis_inner_r, mc_lon);
                    let (x2, y2) = polar_to_xy(cx, cy, axis_outer_r, mc_lon);

                    let extra = format!(
                        "{DATA_RB_STRUCTURE}=\"house-axis\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_DATASET}=\"{dataset_attr}\" {DATA_RB_HOUSE_SET}=\"{house_set_attr}\" {DATA_RB_AXIS}=\"mc\" {DATA_RB_DEG}=\"{:.3}\"",
                        mc_lon
                    );

                    push_line_extra(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        axis_stroke,
                        axis_width,
                        Some("rb-house-axis rb-house-axis-mc"),
                        Some(extra.as_str()),
                    );

                    let hit_w = (axis_width * 6.0).max(14.0);
                    push_hit_line(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        hit_w,
                        "rb-house-axis-hit",
                        Some(extra.as_str()),
                    );
                }

                // IC axis.
                {
                    let (x1, y1) = polar_to_xy(cx, cy, axis_inner_r, ic_lon);
                    let (x2, y2) = polar_to_xy(cx, cy, axis_outer_r, ic_lon);

                    let extra = format!(
                        "{DATA_RB_STRUCTURE}=\"house-axis\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_DATASET}=\"{dataset_attr}\" {DATA_RB_HOUSE_SET}=\"{house_set_attr}\" {DATA_RB_AXIS}=\"ic\" {DATA_RB_DEG}=\"{:.3}\"",
                        ic_lon
                    );

                    push_line_extra(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        axis_stroke,
                        axis_width,
                        Some("rb-house-axis rb-house-axis-ic"),
                        Some(extra.as_str()),
                    );

                    let hit_w = (axis_width * 6.0).max(14.0);
                    push_hit_line(
                        out,
                        x1,
                        y1,
                        x2,
                        y2,
                        hit_w,
                        "rb-house-axis-hit",
                        Some(extra.as_str()),
                    );
                }
            }
        }
    }

    Ok(())
}
