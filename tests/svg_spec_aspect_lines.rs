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
        house_cusps: Vec::new(),
    };

    let svg = rubrum_svg::chart_to_svg_string_spec(&theme, &layout, None, &data)
        .expect("svg spec render failed");

    // Aspect lines are injected as raw SVG with a stable id.
    assert!(svg.contains("id=\"rb-aspects\""));
}
