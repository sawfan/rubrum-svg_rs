#![allow(clippy::too_many_arguments)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::unnecessary_to_owned)]

//! Pure-SVG rendering backend for Rubrum charts.
//!
//! This crate exists to provide a Cairo-free renderer that can compile to `wasm32`.
//!
//! The primary entrypoint is [`chart_to_svg_string_spec`].

mod primitive;
mod spec;

pub use spec::{
    AspectGridSvgGroup, AspectGridSvgOptions, ChartSvgRenderOptions, DeclinationMapSvgGroup,
    DeclinationMapSvgOptions, aspect_grid_to_svg_document, aspect_grid_to_svg_group,
    aspect_grid_to_svg_string, chart_to_svg_string_spec, chart_to_svg_string_spec_with_options,
    declination_map_to_svg_document, declination_map_to_svg_group, declination_map_to_svg_string,
    default_aspect_grid_endpoint_order,
};
