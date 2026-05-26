use rubrum::Sign;
use rubrum_render::core::geometry::{normalize_deg, polar_to_xy};
use rubrum_render::glyphs::sign_svg_symbol_id;
use rubrum_render::layout::BandSpec;
use rubrum_render::metadata::svg_data::{
    DATA_RB_BAND, DATA_RB_DEG, DATA_RB_SIGN_INDEX, DATA_RB_STRUCTURE, StructureKind,
};
use rubrum_render::options::RgbaColor;

use crate::primitive::{
    canonical_key_to_css_token as key_to_css_token, escape_xml_attr, hit_line, line_extra,
    rgba_css_var, text_centered, use_symbol,
};
use rubrum_render::theme::Theme;

use crate::primitive::push_svg_node;

pub fn render_signs(
    out: &mut String,
    theme: &Theme,
    band: &BandSpec,
    cx: f64,
    cy: f64,
    r_inner: f64,
    r_outer: f64,
    rotation_deg: f64,
    default_text_color: RgbaColor,
) {
    let options = &theme.cairo;

    let Some(signs) = band.signs.as_ref() else {
        return;
    };
    if !signs.enabled {
        return;
    }

    const SIGNS: [Sign; 12] = [
        Sign::Aries,
        Sign::Taurus,
        Sign::Gemini,
        Sign::Cancer,
        Sign::Leo,
        Sign::Virgo,
        Sign::Libra,
        Sign::Scorpio,
        Sign::Sagittarius,
        Sign::Capricorn,
        Sign::Aquarius,
        Sign::Pisces,
    ];

    let band_id_attr = escape_xml_attr(band.id.as_str());
    let band_id_token = key_to_css_token(band.id.as_str());

    let base_colors = theme.effective_base_colors();

    let stroke = signs
        .divider_stroke
        .or_else(|| band.boundary.as_ref().and_then(|b| b.color))
        .unwrap_or(base_colors.muted);

    let width = signs.divider_width.unwrap_or(1.0).max(0.5);

    if signs.dividers {
        for i in 0..12 {
            let divider_deg = normalize_deg((i as f64) * 30.0 + rotation_deg);
            let (x1, y1) = polar_to_xy(cx, cy, r_inner, divider_deg);
            let (x2, y2) = polar_to_xy(cx, cy, r_outer, divider_deg);

            let extra = format!(
                "{DATA_RB_STRUCTURE}=\"{}\" {DATA_RB_BAND}=\"{band_id_attr}\" {DATA_RB_SIGN_INDEX}=\"{i}\" {DATA_RB_DEG}=\"{:.3}\"",
                StructureKind::SignDivider.as_str(),
                divider_deg
            );
            let class =
                format!("rb-sign-divider rb-sign-divider-{band_id_token} rb-sign-divider-idx-{i}");

            if let Some(line) = line_extra(
                x1,
                y1,
                x2,
                y2,
                stroke,
                width,
                Some(class.as_str()),
                Some(extra.as_str()),
            ) {
                // Preserve legacy formatting: these nodes were historically emitted without
                // leading indentation.
                push_svg_node(out, "", line);
            }

            // Wide transparent hit target for reliable selection.
            let hit_w = (width * 6.0).max(14.0);
            if let Some(hit) = hit_line(
                x1,
                y1,
                x2,
                y2,
                hit_w,
                "rb-sign-divider-hit",
                Some(extra.as_str()),
            ) {
                out.push_str(hit.to_string().as_str());
                out.push('\n');
            }
        }
    }

    if signs.labels {
        let label_r = (r_inner + r_outer) / 2.0;
        let label_color = default_text_color;

        // Outer-wheel sign labels:
        //
        // Prefer sprite glyphs when a sprite sheet URL is configured (and the theme flag allows
        // it). Unicode zodiac symbols vary widely across platforms/fonts and can fall back to
        // inconsistent glyphs, which manifests as uneven weight, baseline drift, and poor optical
        // centering.
        let sprite_url = if theme.svg.use_sprite_sign_labels {
            theme.svg.glyph_sprite_url.as_deref()
        } else {
            None
        };

        for (i, sign) in SIGNS.iter().copied().enumerate() {
            let label_deg = normalize_deg((i as f64) * 30.0 + 15.0 + rotation_deg);
            let (x, y) = polar_to_xy(cx, cy, label_r, label_deg);

            if let Some(sprite_base) = sprite_url {
                let symbol_id = sign_svg_symbol_id(sign);
                let href = format!("{sprite_base}#{symbol_id}");
                let size = options.sign_font_size.max(1.0) * 1.6;

                // Note: fill/stroke tinting depends on the sprite's internal paint; we still emit
                // color attributes for packs that inherit currentColor-style behavior.
                let color_attr = rgba_css_var("--rb-chart-text", label_color);
                let extra = format!(
                    "data-rb-structure=\"sign-label\" data-rb-sign-index=\"{i}\" data-rb-deg=\"{:.3}\" fill=\"{color_attr}\" stroke=\"{color_attr}\"",
                    label_deg
                );

                if let Some(node) = use_symbol(
                    href.as_str(),
                    x,
                    y,
                    size,
                    "rb-sign-label rb-sign-label-glyph",
                    Some(extra.as_str()),
                ) {
                    out.push_str("  ");
                    out.push_str(node.to_string().as_str());
                    out.push('\n');
                }
            } else {
                let label = sign.symbol_text();
                let node = text_centered(
                    x,
                    y,
                    label.as_str(),
                    label_color,
                    options.font_family.as_str(),
                    options.sign_font_size,
                );
                out.push_str("  ");
                out.push_str(node.to_string().as_str());
                out.push('\n');
            }
        }
    }
}
