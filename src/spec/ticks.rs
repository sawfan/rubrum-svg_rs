use rubrum_render::layout::{TickAnchor, TickDirection};

pub fn tick_direction_token(d: TickDirection) -> &'static str {
    match d {
        TickDirection::Inward => "inward",
        TickDirection::Outward => "outward",
        TickDirection::Both => "both",
    }
}

pub fn tick_anchor_token(a: TickAnchor) -> &'static str {
    match a {
        TickAnchor::Inner => "inner",
        TickAnchor::Outer => "outer",
    }
}
