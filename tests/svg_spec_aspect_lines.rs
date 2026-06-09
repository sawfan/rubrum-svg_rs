use rubrum_render::chart_data::{DatasetMetadata, PlacementMetadata};
use rubrum_render::dataset::{DatasetData, HouseSetData};
use rubrum_render::layout::{
    BandSpec, GlyphLaneMode, GlyphLaneSpec, LaneSpec, PlacementBoundaryTicksSpec,
};
use rubrum_render::style::LaneTemplate;
use rubrum_render::thickness::ThicknessSpec;
use rubrum_render::{
    Body, ChartData, Coordinate, Layout, Motion, Occupant, Placement, PlacementMotion, SignDegree,
    Theme,
};

#[test]
fn svg_spec_renderer_draws_aspect_lines_in_center() {
    let mut theme = Theme::default();
    theme.aspects.enabled = true;
    theme.aspects.stroke.width = Some(2.0);
    theme.aspects.stroke.alpha = Some(1.0);

    // Minimal layout:
    // - aspects anchor lane to opt into aspect computation
    // - bodies lane that actually renders the placements
    // Uses rubrum's default aspect rules.

    let layout = Layout {
        bands: vec![
            BandSpec {
                id: "aspects".to_owned(),
                thickness: ThicknessSpec::Px(120.0),
                lanes: vec![LaneSpec {
                    id: Some("aspects".to_owned()),
                    template: None,
                    dataset: Some("natal".to_owned()),
                    house_set: None,
                    glyphs: Some(GlyphLaneSpec {
                        mode: GlyphLaneMode::Aspects,
                        other_dataset: None,
                        radial_bias: Some(0.5),
                        collision_avoidance: None,
                        placement_ticks: None,
                        placement_boundary_ticks: Some(PlacementBoundaryTicksSpec {
                            enabled: true,
                            anchor: None,
                            direction: None,
                            stroke: None,
                            width: Some(2.0),
                            length_in: Some(10.0),
                            length_out: Some(0.0),
                            offset_in: Some(0.0),
                            offset_out: Some(0.0),
                        }),
                        declination_radial: None,
                        placement_labels: None,
                    }),
                    endpoint_filter: None,
                    overrides: LaneTemplate::default(),
                }],
                fill: None,
                boundary: None,
                ticks_inner: None,
                ticks_outer: None,
                houses: None,
                signs: None,
            },
            BandSpec {
                id: "bodies".to_owned(),
                thickness: ThicknessSpec::Px(220.0),
                lanes: vec![LaneSpec {
                    id: Some("natal".to_owned()),
                    template: None,
                    dataset: Some("natal".to_owned()),
                    house_set: None,
                    glyphs: Some(GlyphLaneSpec {
                        mode: GlyphLaneMode::Bodies,
                        ..Default::default()
                    }),
                    endpoint_filter: None,
                    overrides: LaneTemplate::default(),
                }],
                fill: None,
                boundary: None,
                ticks_inner: None,
                ticks_outer: None,
                houses: None,
                signs: None,
            },
        ],
    };

    // Two placements in exact opposition (0° and 180°) should yield at least one aspect edge.
    let sun = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(0.0)),
            Occupant::Body(Body::Sun),
        ),
        Motion::Direct,
    );

    let mars = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(180.0)),
            Occupant::Body(Body::Mars),
        ),
        Motion::Direct,
    );

    let data = ChartData {
        natal_bodies: Vec::new(),
        datasets: vec![DatasetData {
            id: "natal".to_owned(),
            bodies: vec![sun, mars],
        }],
        house_sets: vec![HouseSetData {
            id: "natal".to_owned(),
            house_cusps: Vec::new(),
        }],
        dataset_metadata: Vec::new(),
        house_cusps: Vec::new(),
    };

    let svg = rubrum_svg::chart_to_svg_string_spec(&theme, &layout, None, &data)
        .expect("svg spec render failed");

    // Aspect lines are injected as raw SVG with a stable id.
    assert!(svg.contains("id=\"rb-aspects\""));
}

#[test]
fn declination_map_svg_renders_rectangular_projection_with_metadata() {
    let sun = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(15.0)),
            Occupant::Body(Body::Sun),
        ),
        Motion::Direct,
    );
    let moon = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(195.0)),
            Occupant::Body(Body::Moon),
        ),
        Motion::Direct,
    );

    let data = ChartData {
        natal_bodies: Vec::new(),
        datasets: vec![DatasetData {
            id: "natal".to_owned(),
            bodies: vec![sun, moon],
        }],
        house_sets: Vec::new(),
        dataset_metadata: vec![DatasetMetadata {
            id: "natal".to_owned(),
            placements: vec![
                PlacementMetadata {
                    occupant: Occupant::Body(Body::Sun),
                    declination_deg: Some(24.2),
                },
                PlacementMetadata {
                    occupant: Occupant::Body(Body::Moon),
                    declination_deg: Some(-3.5),
                },
            ],
        }],
        house_cusps: Vec::new(),
    };

    let svg = rubrum_svg::declination_map_to_svg_string(
        &Theme::default(),
        &data,
        &rubrum_render::DeclinationMapLayout::default(),
        rubrum_svg::DeclinationMapSvgOptions::default(),
    )
    .expect("declination map render failed");

    assert!(svg.contains("rb-declination-map"));
    assert!(svg.contains("rb-declination-map-ecliptic"));
    assert!(svg.contains("data-rb-declination=\"24.2\""));
    assert!(svg.contains("rb-declination-map-placement-oob"));
    assert!(svg.contains("15°00′"));
    assert!(!svg.contains("SUN 15°00′"));
}

#[test]
fn svg_spec_renderer_draws_small_retrograde_marker_with_sprite_or_text_fallback() {
    let layout = Layout {
        bands: vec![BandSpec {
            id: "bodies".to_owned(),
            thickness: ThicknessSpec::Px(220.0),
            lanes: vec![LaneSpec {
                id: Some("natal".to_owned()),
                template: None,
                dataset: Some("natal".to_owned()),
                house_set: None,
                glyphs: Some(GlyphLaneSpec {
                    mode: GlyphLaneMode::Bodies,
                    ..Default::default()
                }),
                endpoint_filter: None,
                overrides: LaneTemplate::default(),
            }],
            fill: None,
            boundary: None,
            ticks_inner: None,
            ticks_outer: None,
            houses: None,
            signs: None,
        }],
    };

    let mercury_rx = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(120.0)),
            Occupant::Body(Body::Mercury),
        ),
        Motion::Retrograde,
    );

    let data = ChartData {
        natal_bodies: Vec::new(),
        datasets: vec![DatasetData {
            id: "natal".to_owned(),
            bodies: vec![mercury_rx],
        }],
        house_sets: Vec::new(),
        dataset_metadata: Vec::new(),
        house_cusps: Vec::new(),
    };

    let mut sprite_theme = Theme::default();
    sprite_theme.svg.glyph_sprite_url = Some("".to_owned());
    sprite_theme.cairo.occupant_symbol_size = 20.0;

    let sprite_svg = rubrum_svg::chart_to_svg_string_spec(&sprite_theme, &layout, None, &data)
        .expect("svg spec render with sprite failed");

    assert!(sprite_svg.contains("href=\"#rb-body-mercury\""));
    assert!(sprite_svg.contains("href=\"#rb-motion-retrograde\""));
    assert!(sprite_svg.contains("rb-motion-retrograde-glyph"));
    assert!(sprite_svg.contains("width=\"9.6\""));
    assert!(!sprite_svg.contains("☿℞"));

    let mut text_theme = Theme::default();
    text_theme.svg.glyph_sprite_url = None;
    text_theme.cairo.label_font_size = 18.0;

    let text_svg = rubrum_svg::chart_to_svg_string_spec(&text_theme, &layout, None, &data)
        .expect("svg spec render with text fallback failed");

    assert!(text_svg.contains("data-rb-occupant=\"mercury\""));
    assert!(text_svg.contains("rb-motion-retrograde-text"));
    assert!(text_svg.contains("font-size=\"11.16\""));
    assert!(!text_svg.contains("☿℞"));
}

#[test]
fn svg_spec_renderer_uses_lot_glyphs_when_available() {
    let layout = Layout {
        bands: vec![BandSpec {
            id: "bodies".to_owned(),
            thickness: ThicknessSpec::Px(220.0),
            lanes: vec![LaneSpec {
                id: Some("natal".to_owned()),
                template: None,
                dataset: Some("natal".to_owned()),
                house_set: None,
                glyphs: Some(GlyphLaneSpec {
                    mode: GlyphLaneMode::Bodies,
                    ..Default::default()
                }),
                endpoint_filter: None,
                overrides: LaneTemplate::default(),
            }],
            fill: None,
            boundary: None,
            ticks_inner: None,
            ticks_outer: None,
            houses: None,
            signs: None,
        }],
    };

    let fortune = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(42.0)),
            Occupant::Lot(rubrum::Lot::Fortune),
        ),
        Motion::Direct,
    );

    let data = ChartData {
        natal_bodies: Vec::new(),
        datasets: vec![DatasetData {
            id: "natal".to_owned(),
            bodies: vec![fortune],
        }],
        house_sets: Vec::new(),
        dataset_metadata: Vec::new(),
        house_cusps: Vec::new(),
    };

    let mut sprite_theme = Theme::default();
    sprite_theme.svg.glyph_sprite_url = Some("".to_owned());

    let sprite_svg = rubrum_svg::chart_to_svg_string_spec(&sprite_theme, &layout, None, &data)
        .expect("svg spec render with lot sprite failed");

    assert!(sprite_svg.contains("href=\"#rb-lot-fortune\""));
    assert!(sprite_svg.contains("data-rb-occupant-type=\"lot\""));

    let mut text_theme = Theme::default();
    text_theme.svg.glyph_sprite_url = None;

    let text_svg = rubrum_svg::chart_to_svg_string_spec(&text_theme, &layout, None, &data)
        .expect("svg spec render with lot text fallback failed");

    assert!(text_svg.contains("data-rb-occupant=\"fortune\""));
    assert!(text_svg.contains("⊗"));
}

#[test]
fn svg_spec_renderer_uses_south_node_glyphs_when_available() {
    let layout = Layout {
        bands: vec![BandSpec {
            id: "bodies".to_owned(),
            thickness: ThicknessSpec::Px(220.0),
            lanes: vec![LaneSpec {
                id: Some("natal".to_owned()),
                template: None,
                dataset: Some("natal".to_owned()),
                house_set: None,
                glyphs: Some(GlyphLaneSpec {
                    mode: GlyphLaneMode::Bodies,
                    ..Default::default()
                }),
                endpoint_filter: None,
                overrides: LaneTemplate::default(),
            }],
            fill: None,
            boundary: None,
            ticks_inner: None,
            ticks_outer: None,
            houses: None,
            signs: None,
        }],
    };

    let true_south_node = PlacementMotion::new(
        Placement::new(
            Coordinate::SignDegree(SignDegree::new(220.0)),
            Occupant::ChartPoint(rubrum::ChartPoint::TrueSouthNode),
        ),
        Motion::Direct,
    );

    let data = ChartData {
        natal_bodies: Vec::new(),
        datasets: vec![DatasetData {
            id: "natal".to_owned(),
            bodies: vec![true_south_node],
        }],
        house_sets: Vec::new(),
        dataset_metadata: Vec::new(),
        house_cusps: Vec::new(),
    };

    let mut sprite_theme = Theme::default();
    sprite_theme.svg.glyph_sprite_url = Some("".to_owned());

    let sprite_svg = rubrum_svg::chart_to_svg_string_spec(&sprite_theme, &layout, None, &data)
        .expect("svg spec render with south-node sprite failed");

    assert!(sprite_svg.contains("href=\"#rb-chart-point-true_south_node\""));

    let mut text_theme = Theme::default();
    text_theme.svg.glyph_sprite_url = None;

    let text_svg = rubrum_svg::chart_to_svg_string_spec(&text_theme, &layout, None, &data)
        .expect("svg spec render with south-node text fallback failed");

    assert!(text_svg.contains("data-rb-occupant=\"true_south_node\""));
    assert!(text_svg.contains("☋"));
}
