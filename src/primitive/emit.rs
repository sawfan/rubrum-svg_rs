use std::fmt::{Display, Write};

pub fn push_svg_node(out: &mut String, indent: &str, node: impl Display) {
    // Emit a single SVG node with stable indentation and a trailing newline.
    //
    // Using `writeln!` avoids an intermediate `to_string()` allocation while preserving
    // the exact textual representation produced by the `svg` crate.
    let _ = writeln!(out, "{indent}{node}");
}
