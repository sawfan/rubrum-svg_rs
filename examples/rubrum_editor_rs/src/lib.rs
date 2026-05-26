use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use js_sys::{Function, Promise, Reflect};
use wasm_bindgen_futures::{JsFuture, spawn_local};

use rubrum_render::core::geometry::polar_to_xy;
use rubrum_render::{ChartData, Coordinate, Layout, RgbaColor, Sign, SignDegree, Theme};
use serde::{Deserialize, Serialize};

use web_sys::{Element, HtmlInputElement, HtmlSelectElement, PointerEvent, SvgsvgElement};

thread_local! {
    static DID_LOG_PRISM_MISSING: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

fn debug_log(msg: &str) {
    if cfg!(debug_assertions) {
        web_sys::console::log_1(&msg.into());
    }
}

fn debug_warn(msg: &str) {
    if cfg!(debug_assertions) {
        web_sys::console::warn_1(&msg.into());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectionKind {
    Chart,
    Placement,
    Aspect,
    Structure,
}

#[derive(Debug, Clone)]
struct Selection {
    kind: SelectionKind,
    attrs: std::collections::BTreeMap<String, String>,

    // Element that owns the most relevant data-rb-* attributes for this selection.
    // We use it for selection highlighting in the injected SVG.
    el: Element,
}

#[derive(Debug, Clone)]
struct DragState {
    pointer_id: i32,
    dataset: String,
    endpoint: String,

    // Cached <svg> element so we can keep using viewBox transforms while dragging.
    svg: SvgsvgElement,

    // Chart geometry (SVG coords).
    cx: f64,
    cy: f64,
    rotation_deg: f64,

    // Keep the dragged placement in the same lane by holding its radius constant.
    r: f64,

    // We accumulate pointer motion as a continuous delta so crossing 0°/360° remains smooth.
    last_pointer_display_deg: f64,
    pointer_display_deg_unwrapped: f64,
    start_pointer_display_deg_unwrapped: f64,

    // Pointer-down offset so the glyph doesn't "jump" if the pointer is not exactly on the
    // glyph/hit-target center.
    offset_display_deg: f64,

    // Whether we have actually moved enough to treat this as a drag (used to suppress click).
    did_move: bool,

    // DOM nodes we mutate live during drag.
    placement_el: Element,
    hit_el: Element,
    glyph_el: Option<Element>,

    // Latest absolute ecliptic longitude (0..360).
    degree: f64,
}

#[derive(Debug, Clone)]
struct SliderDragState {
    pointer_id: i32,
    dataset: String,
    endpoint: String,

    // Cached <svg> element so we can keep using viewBox transforms while scrubbing.
    svg: SvgsvgElement,

    // Chart geometry (SVG coords).
    cx: f64,
    cy: f64,
    rotation_deg: f64,

    // Keep the dragged placement in the same lane by holding its radius constant.
    r: f64,

    // DOM nodes we mutate live during drag.
    placement_el: Element,
    hit_el: Element,
    glyph_el: Option<Element>,

    // Pointer-down slider state.
    start_client_x: f64,
    slider_width_px: f64,
    start_deg: f64,

    // Latest absolute ecliptic longitude (0..360).
    degree: f64,
}

fn parse_dataset_attr(attrs: &std::collections::BTreeMap<String, String>) -> Option<String> {
    attrs.get("data-rb-dataset").cloned()
}

fn parse_endpoint_attr(attrs: &std::collections::BTreeMap<String, String>) -> Option<String> {
    attrs
        .get("data-rb-endpoint")
        .or_else(|| attrs.get("data-rb-occupant"))
        .cloned()
}

#[allow(dead_code)]
fn parse_occupant_type_attr(attrs: &std::collections::BTreeMap<String, String>) -> Option<String> {
    attrs.get("data-rb-occupant-type").cloned()
}

fn selection_summary(selection: &Selection) -> String {
    let kind = match selection.kind {
        SelectionKind::Chart => "chart",
        SelectionKind::Placement => "placement",
        SelectionKind::Aspect => "aspect",
        SelectionKind::Structure => "structure",
    };

    let mut lines = vec![format!("kind: {kind}")];

    if let Some(dataset) = parse_dataset_attr(&selection.attrs) {
        lines.push(format!("dataset: {dataset}"));
    }

    if let Some(endpoint) = parse_endpoint_attr(&selection.attrs) {
        lines.push(format!("endpoint: {endpoint}"));
    }

    if let Some(k) = selection.attrs.get("data-rb-aspect-kind") {
        lines.push(format!("aspect_kind: {k}"));
    }

    if let Some(a) = selection.attrs.get("data-rb-aspect-a") {
        lines.push(format!("a: {a}"));
    }

    if let Some(b) = selection.attrs.get("data-rb-aspect-b") {
        lines.push(format!("b: {b}"));
    }

    if let Some(d) = selection.attrs.get("data-rb-degree") {
        lines.push(format!("degree: {d}"));
    }

    if let Some(retro) = selection.attrs.get("data-rb-retrograde") {
        lines.push(format!("retrograde: {retro}"));
    }

    if let Some(s) = selection.attrs.get("data-rb-structure") {
        lines.push(format!("structure: {s}"));
    }
    if let Some(band) = selection.attrs.get("data-rb-band") {
        lines.push(format!("band: {band}"));
    }
    if let Some(lane_id) = selection.attrs.get("data-rb-lane-id") {
        lines.push(format!("lane_id: {lane_id}"));
    }
    if let Some(lane_idx) = selection.attrs.get("data-rb-lane-index") {
        lines.push(format!("lane_index: {lane_idx}"));
    }

    // Include a couple extra keys if present.
    for key in ["data-rb-occupant-type", "data-rb-occupant"].iter() {
        if let Some(v) = selection.attrs.get(*key) {
            lines.push(format!("{key}: {v}"));
        }
    }

    lines.join("\n")
}

#[derive(Debug, Deserialize, Serialize)]
struct ThemeFile {
    theme: Theme,
}

#[derive(Debug, Deserialize)]
struct LayoutFile {
    layout: Layout,
}

#[derive(Debug, Deserialize, Serialize)]
struct DataFile {
    data: ChartData,
}

#[derive(Debug, Deserialize, Serialize)]
struct AspectsFile {
    rules: rubrum_render::AspectRules,
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn set_inline_style_property(el: &Element, key: &str, value: &str) {
    // Avoid `HtmlElement::style()` so we don't need extra `web-sys` feature flags.
    // Instead, update the inline `style` attribute in a way that preserves other properties.
    let key = key.trim().to_ascii_lowercase();

    let existing = el.get_attribute("style").unwrap_or_default();
    let mut out: Vec<(String, String)> = Vec::new();

    for part in existing.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        let Some((k, v)) = part.split_once(':') else {
            continue;
        };

        let k_norm = k.trim().to_ascii_lowercase();
        if k_norm == key {
            // Drop the existing value; we will append the updated one below.
            continue;
        }

        out.push((k_norm, v.trim().to_owned()));
    }

    out.push((key, value.trim().to_owned()));

    let style = out
        .into_iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("; ");

    let _ = el.set_attribute("style", format!("{style};").as_str());
}

fn prism_highlight_element(el: &Element) {
    // Best-effort: if Prism isn't loaded yet, fall back to plain escaped text so the
    // highlight <pre><code> layer still shows content.
    let global = js_sys::global();

    let prism = match Reflect::get(&global, &JsValue::from_str("Prism")) {
        Ok(v) => v,
        Err(_) => {
            DID_LOG_PRISM_MISSING.with(|did| {
                if !did.get() {
                    did.set(true);
                    debug_warn(
                        "rubrum_cairo wasm: Prism missing; using plain-text highlighting fallback",
                    );
                }
            });

            // Prism missing: use a plain-text fallback.
            if let Some(text) = el.text_content() {
                el.set_inner_html(&escape_html(&text));
            }
            return;
        }
    };

    if prism.is_null() || prism.is_undefined() {
        DID_LOG_PRISM_MISSING.with(|did| {
            if !did.get() {
                did.set(true);
                debug_warn(
                    "rubrum_cairo wasm: Prism undefined; using plain-text highlighting fallback",
                );
            }
        });

        if let Some(text) = el.text_content() {
            el.set_inner_html(&escape_html(&text));
        }
        return;
    }

    let highlight_element = match Reflect::get(&prism, &JsValue::from_str("highlightElement")) {
        Ok(v) => v,
        Err(_) => {
            if let Some(text) = el.text_content() {
                el.set_inner_html(&escape_html(&text));
            }
            return;
        }
    };

    let Ok(f) = highlight_element.dyn_into::<Function>() else {
        if let Some(text) = el.text_content() {
            el.set_inner_html(&escape_html(&text));
        }
        return;
    };

    let _ = f.call1(&prism, &JsValue::from(el.clone()));
}

fn set_code_highlight(document: &web_sys::Document, code_id: &str, text: &str) {
    let Some(el) = document.get_element_by_id(code_id) else {
        return;
    };

    // Set raw text, then let Prism tokenize + escape.
    el.set_text_content(Some(text));
    prism_highlight_element(&el);
}

fn sync_code_editor_scroll_from_textarea(
    document: &web_sys::Document,
    textarea_id: &str,
    code_id: &str,
) {
    let Some(ta_el) = document.get_element_by_id(textarea_id) else {
        return;
    };
    let Ok(ta) = ta_el.dyn_into::<web_sys::HtmlTextAreaElement>() else {
        return;
    };

    // We translate the <code> layer instead of scrolling the <pre>.
    // This avoids cases where the <pre> isn't actually scrollable (or doesn't repaint) even
    // though the textarea is scrolling.
    let Some(code_el) = document.get_element_by_id(code_id) else {
        return;
    };
    let Ok(code) = code_el.dyn_into::<Element>() else {
        return;
    };

    let top = ta.scroll_top();
    let left = ta.scroll_left();

    // Translate content so the visible highlight matches the textarea's scrolled viewport.
    // Note: scroll offsets are integer pixels in web-sys.
    let transform = format!("translate({}px, {}px)", -left, -top);
    set_inline_style_property(&code, "transform", transform.as_str());
}

fn sync_code_editor_scroll_from_highlight(
    document: &web_sys::Document,
    textarea_id: &str,
    code_id: &str,
) {
    let Some(code_el) = document.get_element_by_id(code_id) else {
        return;
    };
    let Some(pre_el) = code_el.parent_element() else {
        return;
    };
    let Ok(pre) = pre_el.dyn_into::<web_sys::HtmlElement>() else {
        return;
    };

    // With the translate-based model, the textarea is the canonical scroll owner.
    // If the highlight layer scrolls for any reason, snap it back and re-apply the textarea
    // scroll-derived translation.
    if pre.scroll_top() != 0 {
        pre.set_scroll_top(0);
    }

    if pre.scroll_left() != 0 {
        pre.set_scroll_left(0);
    }

    sync_code_editor_scroll_from_textarea(document, textarea_id, code_id);
}

// --- Properties panel (Chart-wide UI -> Theme TOML) ---

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiColorMode {
    Light,
    Dark,
}

#[derive(Debug, Clone)]
struct UiThemeOptions {
    color_mode: UiColorMode,
    background: RgbaColor,
    foreground: RgbaColor,
    muted: RgbaColor,

    aspects_enabled: bool,

    glyph_sprite_url: String,
    use_sprite_sign_labels: bool,
}

fn clamp01(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 1.0)
}

fn rgba_to_hex_rgb(c: RgbaColor) -> String {
    fn to_u8(v: f64) -> u8 {
        (clamp01(v) * 255.0).round().clamp(0.0, 255.0) as u8
    }

    let r = to_u8(c.r);
    let g = to_u8(c.g);
    let b = to_u8(c.b);

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn hex_rgb_to_rgba(s: &str, alpha: f64) -> Option<RgbaColor> {
    let s = s.trim();
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;

    Some(RgbaColor {
        r: (r as f64) / 255.0,
        g: (g as f64) / 255.0,
        b: (b as f64) / 255.0,
        a: clamp01(alpha),
    })
}

fn ui_theme_from_theme(theme: &Theme) -> UiThemeOptions {
    // Prefer the active base palette if color_mode exists; else use Theme defaults.
    let base = theme.effective_base_colors();

    let color_mode = match theme
        .color_mode
        .as_ref()
        .map(|m| m.mode)
        .unwrap_or(rubrum_render::theme::ColorMode::Light)
    {
        rubrum_render::theme::ColorMode::Dark => UiColorMode::Dark,
        rubrum_render::theme::ColorMode::Light => UiColorMode::Light,
    };

    UiThemeOptions {
        color_mode,
        background: base.background,
        foreground: base.foreground,
        muted: base.muted,
        aspects_enabled: theme.aspects.enabled,
        glyph_sprite_url: theme.svg.glyph_sprite_url.clone().unwrap_or_default(),
        use_sprite_sign_labels: theme.svg.use_sprite_sign_labels,
    }
}

// --- Advanced drawer (developer / power-user UI) ---

fn set_adv_tab_active(document: &web_sys::Document, tab: &str) {
    // Tabs
    if let Ok(list) = document.query_selector_all(".advanced-drawer .tab") {
        let len = list.length();
        for i in 0..len {
            let Some(node) = list.item(i) else {
                continue;
            };
            let Ok(el) = node.dyn_into::<Element>() else {
                continue;
            };

            let is_active = el
                .get_attribute("data-rb-adv-tab")
                .as_deref()
                .is_some_and(|v| v == tab);

            if is_active {
                let _ = el.class_list().add_1("active");
            } else {
                let _ = el.class_list().remove_1("active");
            }
        }
    }

    // Sections
    for section in [
        ("theme", "adv_section_theme"),
        ("layout", "adv_section_layout"),
        ("data", "adv_section_data"),
        ("aspects", "adv_section_aspects"),
        ("export", "adv_section_export"),
    ] {
        let (key, id) = section;
        if let Some(el) = document.get_element_by_id(id) {
            if key == tab {
                let _ = el.class_list().add_1("active");
            } else {
                let _ = el.class_list().remove_1("active");
            }
        }
    }
}

fn set_advanced_drawer_open(document: &web_sys::Document, open: bool) {
    let Some(drawer) = document.get_element_by_id("advanced_drawer") else {
        return;
    };

    let _ = drawer.set_attribute("data-rb-open", if open { "1" } else { "0" });

    // When opening, ensure we have a default active tab.
    if open {
        // If none is active, activate Theme.
        let any_active = drawer
            .query_selector(".advanced-drawer-header .tab.active")
            .ok()
            .flatten()
            .is_some();

        if !any_active {
            set_adv_tab_active(document, "theme");
        }
    }
}

fn toggle_advanced_drawer(document: &web_sys::Document) {
    let Some(drawer) = document.get_element_by_id("advanced_drawer") else {
        return;
    };

    let open = drawer
        .get_attribute("data-rb-open")
        .as_deref()
        .unwrap_or("0")
        == "1";

    set_advanced_drawer_open(document, !open);
}

fn setup_advanced_drawer(document: &web_sys::Document) {
    let Some(drawer) = document.get_element_by_id("advanced_drawer") else {
        return;
    };

    // Avoid double-binding if we re-run setup during Prism retries.
    if drawer.get_attribute("data-rb-advanced-bound").as_deref() == Some("1") {
        return;
    }
    let _ = drawer.set_attribute("data-rb-advanced-bound", "1");

    // Default tab
    set_adv_tab_active(document, "theme");

    // Toggle buttons (topbar + bottom bar)
    for id in ["advanced_toggle", "advanced_toggle_bottom"] {
        let Some(btn) = document.get_element_by_id(id) else {
            continue;
        };
        let doc = document.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            toggle_advanced_drawer(&doc);
        });
        let _ = btn.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Close button
    if let Some(btn) = document.get_element_by_id("advanced_close") {
        let doc = document.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            set_advanced_drawer_open(&doc, false);
        });
        let _ = btn.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Tab clicks
    for (id, tab) in [
        ("adv_tab_theme", "theme"),
        ("adv_tab_layout", "layout"),
        ("adv_tab_data", "data"),
        ("adv_tab_aspects", "aspects"),
        ("adv_tab_export", "export"),
    ] {
        let Some(el) = document.get_element_by_id(id) else {
            continue;
        };

        let doc = document.clone();
        let tab = tab.to_owned();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            set_adv_tab_active(&doc, tab.as_str());
        });

        let _ = el.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }
}

fn apply_ui_theme_to_theme(theme: &mut Theme, ui: &UiThemeOptions) {
    // Ensure a color_mode selector exists.
    if theme.color_mode.is_none() {
        theme.color_mode = Some(rubrum_render::theme::ColorModeSelector::default());
    }

    if let Some(sel) = theme.color_mode.as_mut() {
        sel.mode = match ui.color_mode {
            UiColorMode::Light => rubrum_render::theme::ColorMode::Light,
            UiColorMode::Dark => rubrum_render::theme::ColorMode::Dark,
        };

        // Update the active palette table so serialization stays intuitive.
        let base = rubrum_render::theme::BaseColors {
            background: ui.background,
            foreground: ui.foreground,
            muted: ui.muted,
        };

        match sel.mode {
            rubrum_render::theme::ColorMode::Light => {
                sel.light = Some(base);
            }
            rubrum_render::theme::ColorMode::Dark => {
                sel.dark = Some(base);
            }
        }
    }

    theme.aspects.enabled = ui.aspects_enabled;

    let sprite_url = ui.glyph_sprite_url.trim();
    theme.svg.glyph_sprite_url = if sprite_url.is_empty() {
        None
    } else {
        Some(sprite_url.to_owned())
    };
    theme.svg.use_sprite_sign_labels = ui.use_sprite_sign_labels;
}

fn set_properties_status(document: &web_sys::Document, msg: &str) {
    set_text_by_id(document, "prop_status", msg);
}

fn set_properties_from_ui(document: &web_sys::Document, ui: &UiThemeOptions) {
    // Mode
    if let Some(el) = document.get_element_by_id("pc_color_mode") {
        if let Ok(select) = el.dyn_into::<HtmlSelectElement>() {
            let v = match ui.color_mode {
                UiColorMode::Dark => "dark",
                UiColorMode::Light => "light",
            };
            select.set_value(v);
        }
    }

    // Aspects enabled
    if let Some(el) = document.get_element_by_id("pc_aspects_enabled") {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_checked(ui.aspects_enabled);
        }
    }

    // Colors
    if let Some(el) = document.get_element_by_id("pc_bg") {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value(rgba_to_hex_rgb(ui.background).as_str());
        }
    }
    if let Some(el) = document.get_element_by_id("pc_fg") {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value(rgba_to_hex_rgb(ui.foreground).as_str());
        }
    }
    if let Some(el) = document.get_element_by_id("pc_muted") {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value(rgba_to_hex_rgb(ui.muted).as_str());
        }
    }

    // Sprite config
    if let Some(el) = document.get_element_by_id("pc_use_sprite_sign_labels") {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_checked(ui.use_sprite_sign_labels);
        }
    }
    if let Some(el) = document.get_element_by_id("pc_glyph_sprite_url") {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value(ui.glyph_sprite_url.as_str());
        }
    }
}

fn ui_theme_from_properties_controls(
    document: &web_sys::Document,
    base: &UiThemeOptions,
) -> Result<UiThemeOptions, String> {
    let color_mode = match document
        .get_element_by_id("pc_color_mode")
        .and_then(|el| el.dyn_into::<HtmlSelectElement>().ok())
        .map(|s| s.value())
        .unwrap_or_else(|| match base.color_mode {
            UiColorMode::Dark => "dark".to_owned(),
            UiColorMode::Light => "light".to_owned(),
        })
        .as_str()
    {
        "light" => UiColorMode::Light,
        _ => UiColorMode::Dark,
    };

    let aspects_enabled = document
        .get_element_by_id("pc_aspects_enabled")
        .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(base.aspects_enabled);

    let bg = document
        .get_element_by_id("pc_bg")
        .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.value())
        .and_then(|v| hex_rgb_to_rgba(v.as_str(), base.background.a))
        .unwrap_or(base.background);

    let fg = document
        .get_element_by_id("pc_fg")
        .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.value())
        .and_then(|v| hex_rgb_to_rgba(v.as_str(), base.foreground.a))
        .unwrap_or(base.foreground);

    let muted = document
        .get_element_by_id("pc_muted")
        .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.value())
        .and_then(|v| hex_rgb_to_rgba(v.as_str(), base.muted.a))
        .unwrap_or(base.muted);

    let use_sprite_sign_labels = document
        .get_element_by_id("pc_use_sprite_sign_labels")
        .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.checked())
        .unwrap_or(base.use_sprite_sign_labels);

    let glyph_sprite_url = document
        .get_element_by_id("pc_glyph_sprite_url")
        .and_then(|el| el.dyn_into::<HtmlInputElement>().ok())
        .map(|i| i.value())
        .unwrap_or_else(|| base.glyph_sprite_url.clone());

    Ok(UiThemeOptions {
        color_mode,
        background: bg,
        foreground: fg,
        muted,
        aspects_enabled,
        glyph_sprite_url,
        use_sprite_sign_labels,
    })
}

fn apply_properties_to_theme_toml(document: &web_sys::Document) -> Result<(), String> {
    let theme_toml = textarea_value(document, "theme_toml")?;

    let mut theme_file: ThemeFile =
        toml::from_str(theme_toml.as_str()).map_err(|e| format!("theme TOML parse error: {e}"))?;

    let base_ui = ui_theme_from_theme(&theme_file.theme);
    let ui = ui_theme_from_properties_controls(document, &base_ui)?;

    apply_ui_theme_to_theme(&mut theme_file.theme, &ui);

    let new_toml = toml::to_string_pretty(&theme_file)
        .map_err(|e| format!("theme TOML serialize error: {e}"))?;

    set_textarea_value(document, "theme_toml", new_toml.as_str())?;
    dispatch_input_event(document, "theme_toml");

    set_properties_status(document, "Applied.");
    Ok(())
}

fn sync_properties_from_theme_toml(document: &web_sys::Document) {
    let theme_toml = match textarea_value(document, "theme_toml") {
        Ok(v) => v,
        Err(_) => return,
    };

    let theme_file: ThemeFile = match toml::from_str(theme_toml.as_str()) {
        Ok(v) => v,
        Err(_) => return,
    };

    let ui = ui_theme_from_theme(&theme_file.theme);
    set_properties_from_ui(document, &ui);
}

fn set_properties_tab_active(document: &web_sys::Document, tab: &str) {
    // Tabs
    if let Ok(list) = document.query_selector_all(".tabs .tab") {
        let len = list.length();
        for i in 0..len {
            let Some(node) = list.item(i) else {
                continue;
            };
            let Ok(el) = node.dyn_into::<Element>() else {
                continue;
            };

            let is_active = el
                .get_attribute("data-rb-prop-tab")
                .as_deref()
                .is_some_and(|v| v == tab);

            if is_active {
                let _ = el.class_list().add_1("active");
            } else {
                let _ = el.class_list().remove_1("active");
            }
        }
    }

    // Sections
    for section in [
        ("chart", "prop_section_chart"),
        ("selection", "prop_section_selection"),
        ("aspects", "prop_section_aspects"),
    ] {
        let (key, id) = section;
        if let Some(el) = document.get_element_by_id(id) {
            if key == tab {
                let _ = el.class_list().add_1("active");
            } else {
                let _ = el.class_list().remove_1("active");
            }
        }
    }
}

fn setup_properties_panel(document: &web_sys::Document) {
    // If the UI isn't present, do nothing.
    let Some(panel) = document.get_element_by_id("properties_panel") else {
        return;
    };

    // Avoid double-binding during Prism retries.
    if panel.get_attribute("data-rb-properties-bound").as_deref() == Some("1") {
        return;
    }
    let _ = panel.set_attribute("data-rb-properties-bound", "1");

    // Default tab.
    set_properties_tab_active(document, "chart");

    // Initial sync from theme TOML.
    sync_properties_from_theme_toml(document);

    // Bind tab clicks.
    for (id, tab) in [
        ("prop_tab_chart", "chart"),
        ("prop_tab_selection", "selection"),
        ("prop_tab_aspects", "aspects"),
    ] {
        let Some(el) = document.get_element_by_id(id) else {
            continue;
        };

        let doc = document.clone();
        let tab = tab.to_owned();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            set_properties_tab_active(&doc, tab.as_str());
        });

        let _ = el.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Helper to bind one theme-affecting control.
    let bind = |id: &str, event: &str| {
        let Some(el) = document.get_element_by_id(id) else {
            return;
        };

        let doc = document.clone();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            if let Err(e) = apply_properties_to_theme_toml(&doc) {
                set_properties_status(&doc, format!("Error: {e}").as_str());
            }
        });

        let _ = el.add_event_listener_with_callback(event, cb.as_ref().unchecked_ref());
        cb.forget();
    };

    bind("pc_color_mode", "change");
    bind("pc_aspects_enabled", "change");
    bind("pc_use_sprite_sign_labels", "change");

    for id in ["pc_bg", "pc_fg", "pc_muted", "pc_glyph_sprite_url"] {
        bind(id, "input");
    }
}

fn setup_code_editor_highlighting(document: &web_sys::Document, textarea_id: &str, code_id: &str) {
    // Initial fill.
    if let Ok(v) = textarea_value(document, textarea_id) {
        set_code_highlight(document, code_id, v.as_str());
        sync_code_editor_scroll_from_textarea(document, textarea_id, code_id);
    }

    let Some(ta_el) = document.get_element_by_id(textarea_id) else {
        return;
    };

    // Avoid installing duplicate listeners if we re-run highlighting during Prism load retries.
    if ta_el.get_attribute("data-rb-highlight-bound").as_deref() == Some("1") {
        return;
    }

    let _ = ta_el.set_attribute("data-rb-highlight-bound", "1");

    // Update tokens on input.
    {
        let document_for_input = document.clone();
        let textarea_id = textarea_id.to_owned();
        let code_id = code_id.to_owned();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            let Ok(v) = textarea_value(&document_for_input, textarea_id.as_str()) else {
                return;
            };

            set_code_highlight(&document_for_input, code_id.as_str(), v.as_str());
            sync_code_editor_scroll_from_textarea(
                &document_for_input,
                textarea_id.as_str(),
                code_id.as_str(),
            );
        });

        let _ = ta_el.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Keep the highlight layer scroll position in sync when the user scrolls the textarea.
    {
        let document_for_scroll = document.clone();
        let textarea_id = textarea_id.to_owned();
        let code_id = code_id.to_owned();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            sync_code_editor_scroll_from_textarea(
                &document_for_scroll,
                textarea_id.as_str(),
                code_id.as_str(),
            );
        });

        let _ = ta_el.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Some browsers / non-standard scroll behaviors (notably middle-mouse auto-scroll) can scroll
    // the underlying highlight layer instead of the overlay textarea. Keep those in sync too.
    {
        let Some(code_el) = document.get_element_by_id(code_id) else {
            return;
        };
        let Some(pre_el) = code_el.parent_element() else {
            return;
        };

        // Avoid duplicate listener installs.
        if pre_el
            .get_attribute("data-rb-highlight-scroll-bound")
            .as_deref()
            != Some("1")
        {
            let _ = pre_el.set_attribute("data-rb-highlight-scroll-bound", "1");

            let document_for_scroll = document.clone();
            let textarea_id = textarea_id.to_owned();
            let code_id = code_id.to_owned();

            let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
                sync_code_editor_scroll_from_highlight(
                    &document_for_scroll,
                    textarea_id.as_str(),
                    code_id.as_str(),
                );
            });

            let _ = pre_el.add_event_listener_with_callback("scroll", cb.as_ref().unchecked_ref());
            cb.forget();
        }
    }
}

fn setup_prism_highlighting(document: &web_sys::Document) {
    setup_code_editor_highlighting(document, "theme_toml", "theme_toml_highlight");
    setup_code_editor_highlighting(document, "layout_toml", "layout_toml_highlight");
    setup_code_editor_highlighting(document, "data_toml", "data_toml_highlight");
    setup_code_editor_highlighting(document, "aspects_toml", "aspects_toml_highlight");

    // The LLM export highlight is driven by Rust after each render.
    if let Ok(v) = textarea_value(document, "llm_export") {
        set_code_highlight(document, "llm_export_highlight", v.as_str());
    }

    // Enable the visual overlay mode: hide textarea glyphs (but keep caret/selection) so the
    // highlighted <pre><code> is what the user sees.
    enable_editor_highlight_overlay(document);
}

fn enable_editor_highlight_overlay(document: &web_sys::Document) {
    // Only enable overlay-mode for editable editors.
    // The LLM export panel is a read-only `<pre><code>` that must remain scrollable.
    let Ok(list) = document.query_selector_all(".code-editor:not(.code-editor-readonly)") else {
        return;
    };

    let len = list.length();
    for i in 0..len {
        let Some(node) = list.item(i) else {
            continue;
        };
        let Ok(el) = node.dyn_into::<Element>() else {
            continue;
        };
        let _ = el.class_list().add_1("highlight-on");
    }
}

fn prism_ready() -> bool {
    let global = js_sys::global();

    let prism = match Reflect::get(&global, &JsValue::from_str("Prism")) {
        Ok(v) => v,
        Err(_) => return false,
    };

    if prism.is_null() || prism.is_undefined() {
        return false;
    }

    // Ensure the shim exposes highlightElement.
    let highlight_element = match Reflect::get(&prism, &JsValue::from_str("highlightElement")) {
        Ok(v) => v,
        Err(_) => return false,
    };

    if highlight_element.dyn_into::<Function>().is_err() {
        return false;
    }

    // Ensure required languages are registered.
    let languages = match Reflect::get(&prism, &JsValue::from_str("languages")) {
        Ok(v) => v,
        Err(_) => return false,
    };

    if languages.is_null() || languages.is_undefined() {
        return false;
    }

    // In our minimal Prism shim, language definitions are registered as functions:
    // `Prism.languages["toml"] = fn(text) -> html`.
    for lang in ["toml", "markdown"] {
        let v = match Reflect::get(&languages, &JsValue::from_str(lang)) {
            Ok(v) => v,
            Err(_) => return false,
        };

        if v.is_null() || v.is_undefined() {
            return false;
        }

        if v.dyn_into::<Function>().is_err() {
            return false;
        }
    }

    true
}

fn schedule_prism_setup_retry(window: web_sys::Window, document: web_sys::Document, attempt: u32) {
    if prism_ready() {
        web_sys::console::log_1(
            &format!("rubrum_editor wasm: Prism ready after {attempt} retries").into(),
        );
        setup_prism_highlighting(&document);
        return;
    }

    if attempt >= 50 {
        web_sys::console::warn_1(
            &"rubrum_editor wasm: Prism not ready after retries; leaving editors un-highlighted"
                .into(),
        );
        return;
    }

    let window_next = window.clone();
    let document_next = document.clone();

    let cb = Closure::<dyn FnMut()>::new(move || {
        schedule_prism_setup_retry(window_next.clone(), document_next.clone(), attempt + 1);
    });

    // Keep the delay small so it converges quickly, but don't spin.
    let _ = window
        .set_timeout_with_callback_and_timeout_and_arguments_0(cb.as_ref().unchecked_ref(), 50);

    cb.forget();
}

fn house_number(h: rubrum_render::House) -> usize {
    match h {
        rubrum_render::House::First => 1,
        rubrum_render::House::Second => 2,
        rubrum_render::House::Third => 3,
        rubrum_render::House::Fourth => 4,
        rubrum_render::House::Fifth => 5,
        rubrum_render::House::Sixth => 6,
        rubrum_render::House::Seventh => 7,
        rubrum_render::House::Eighth => 8,
        rubrum_render::House::Ninth => 9,
        rubrum_render::House::Tenth => 10,
        rubrum_render::House::Eleventh => 11,
        rubrum_render::House::Twelfth => 12,
    }
}

fn house_label(h: rubrum_render::House) -> String {
    format!("House {}", house_number(h))
}

fn deg_in_range(start: f64, end: f64, d: f64) -> bool {
    let start = normalize_deg_0_360(start);
    let end = normalize_deg_0_360(end);
    let d = normalize_deg_0_360(d);

    if (start - end).abs() < 1e-9 {
        // Degenerate: treat as "no range".
        return false;
    }

    if end > start {
        d >= start && d < end
    } else {
        // Wrap-around across 360°.
        d >= start || d < end
    }
}

fn fmt_sign_deg(deg: f64) -> String {
    let d = normalize_deg_0_360(deg);
    let (sign, in_sign) = degrees_to_sign_parts(d);

    // Keep this stable and LLM-friendly.
    // We keep a couple decimals of in-sign degree rather than trying to preserve the original
    // minutes/seconds fields.
    format!("{:.4}° ({} {:.4}°)", d, format!("{sign:?}"), in_sign)
}

fn chart_house_cusps_abs_deg(
    data: &ChartData,
    house_set_id: &str,
) -> Vec<(rubrum_render::House, f64)> {
    let cusps = data
        .house_set_cusps(house_set_id)
        .unwrap_or_else(|| data.house_cusps.as_slice());

    let mut out: Vec<(rubrum_render::House, f64)> = cusps
        .iter()
        .copied()
        .map(|c| (c.house, c.sign_degree.degrees))
        .collect();

    out.sort_by_key(|(h, _)| house_number(*h));
    out
}

fn house_for_deg(cusps: &[(rubrum_render::House, f64)], deg: f64) -> Option<rubrum_render::House> {
    if cusps.len() < 2 {
        return None;
    }

    // Expect one cusp per house; if not present, just do the best we can.
    for i in 0..cusps.len() {
        let (house, start) = cusps[i];
        let end = cusps[(i + 1) % cusps.len()].1;
        if deg_in_range(start, end, deg) {
            return Some(house);
        }
    }

    None
}

fn aspects_from_dom(document: &web_sys::Document) -> Vec<(String, String, String)> {
    let Some(chart) = document.get_element_by_id("chart") else {
        return Vec::new();
    };

    let Ok(list) = chart.query_selector_all("#rb-aspects .rb-aspect") else {
        return Vec::new();
    };

    let mut out: Vec<(String, String, String)> = Vec::new();

    let len = list.length();
    for i in 0..len {
        let Some(node) = list.item(i) else {
            continue;
        };
        let Ok(el) = node.dyn_into::<Element>() else {
            continue;
        };

        let kind = el
            .get_attribute("data-rb-aspect-kind")
            .unwrap_or_else(|| "unknown".to_owned());
        let a = el
            .get_attribute("data-rb-aspect-a")
            .unwrap_or_else(|| "?".to_owned());
        let b = el
            .get_attribute("data-rb-aspect-b")
            .unwrap_or_else(|| "?".to_owned());

        out.push((kind, a, b));
    }

    out.sort();
    out
}

fn build_llm_export_text(
    theme_toml: &str,
    layout_toml: &str,
    data_toml: &str,
    document: &web_sys::Document,
) -> Result<String, String> {
    let theme_file: ThemeFile =
        toml::from_str(theme_toml).map_err(|e| format!("theme TOML parse error: {e}"))?;

    let layout_file: LayoutFile =
        toml::from_str(layout_toml).map_err(|e| format!("layout TOML parse error: {e}"))?;

    let data_file: DataFile =
        toml::from_str(data_toml).map_err(|e| format!("data TOML parse error: {e}"))?;

    let theme = theme_file.theme;
    let layout = layout_file.layout;
    let data = data_file.data;

    let mut out = String::new();

    out.push_str("# Chart export (LLM-friendly)\n\n");

    out.push_str("## Context\n");
    out.push_str("- Source: rubrum_editor Trunk WASM SVG example\n");
    out.push_str("- Coordinates: ecliptic longitude in degrees (0..360)\n");
    out.push_str("- Houses: derived from configured house cusps (if present)\n\n");

    // Determine which house set to show.
    // Prefer an explicit band houses spec, else a lane binding, else default to "natal".
    let mut house_set_id: Option<String> = None;
    for band in layout.bands.iter() {
        if let Some(houses) = band.houses.as_ref() {
            house_set_id = houses
                .house_set
                .clone()
                .or_else(|| Some("natal".to_owned()));
            break;
        }
    }
    if house_set_id.is_none() {
        for band in layout.bands.iter() {
            for lane in band.lanes.iter() {
                if lane.house_set.is_some() {
                    house_set_id = lane.house_set.clone();
                    break;
                }
            }
            if house_set_id.is_some() {
                break;
            }
        }
    }

    let house_set_id = house_set_id.unwrap_or_else(|| "natal".to_owned());

    let cusps = chart_house_cusps_abs_deg(&data, house_set_id.as_str());

    out.push_str("## House cusps\n");
    out.push_str(&format!("- house_set: {house_set_id}\n"));
    if cusps.is_empty() {
        out.push_str("- (no cusps provided)\n\n");
    } else {
        for (house, deg) in cusps.iter().copied() {
            out.push_str(&format!(
                "- {}: {}\n",
                house_label(house),
                fmt_sign_deg(deg)
            ));
        }
        out.push_str("\n");
    }

    out.push_str("## Placements\n");

    // List datasets in stable order.
    let mut datasets = data.datasets.clone();
    datasets.sort_by(|a, b| a.id.cmp(&b.id));

    // Back-compat: if no datasets, fall back to natal_bodies as "natal".
    if datasets.is_empty() && !data.natal_bodies.is_empty() {
        datasets.push(rubrum_render::DatasetData {
            id: "natal".to_owned(),
            bodies: data.natal_bodies.clone(),
        });
    }

    for ds in datasets.iter() {
        out.push_str(&format!("### Dataset: {}\n", ds.id));

        let mut pms = ds.bodies.clone();
        pms.sort_by(|a, b| {
            let ad = a
                .coordinate()
                .sign_degree()
                .map(|sd| sd.degrees)
                .unwrap_or(0.0);
            let bd = b
                .coordinate()
                .sign_degree()
                .map(|sd| sd.degrees)
                .unwrap_or(0.0);
            ad.partial_cmp(&bd).unwrap_or(std::cmp::Ordering::Equal)
        });

        for pm in pms.iter() {
            let endpoint = pm.occupant().canonical_key();
            let label = rubrum_render::glyphs::occupant_label(pm.occupant());
            let motion = format!("{:?}", pm.motion);

            let Some(sd) = pm.coordinate().sign_degree() else {
                out.push_str(&format!(
                    "- {} ({label}): (non-zodiac coordinate), motion={motion}\n",
                    endpoint
                ));
                continue;
            };

            let deg = sd.degrees;
            let house = house_for_deg(cusps.as_slice(), deg);
            let house_s = house
                .map(house_label)
                .unwrap_or_else(|| "(unknown house)".to_owned());

            out.push_str(&format!(
                "- {} ({label}): {}, {}, motion={}\n",
                endpoint,
                fmt_sign_deg(deg),
                house_s,
                motion
            ));
        }

        out.push_str("\n");
    }

    out.push_str("## Aspects\n");
    out.push_str(&format!("- enabled: {}\n", theme.aspects.enabled));
    out.push_str("- rendered_edges:\n");

    // Prefer reporting what is *actually rendered* (DOM-driven), but enrich each edge with
    // endpoint degrees and the absolute separation.
    //
    // Note: separating/applying (conjoining vs separating) will be added in the future once
    // endpoint speed is recorded reliably.

    // Build a lookup of endpoint -> absolute ecliptic degrees (0..360).
    let mut endpoint_deg: std::collections::BTreeMap<String, f64> =
        std::collections::BTreeMap::new();

    for ds in data.datasets.iter() {
        for pm in ds.bodies.iter() {
            let Some(sd) = pm.coordinate().sign_degree() else {
                continue;
            };
            endpoint_deg.insert(pm.occupant().canonical_key().to_owned(), sd.degrees);
        }
    }

    // Back-compat: include the legacy natal list as well.
    for pm in data.natal_bodies.iter() {
        let Some(sd) = pm.coordinate().sign_degree() else {
            continue;
        };
        endpoint_deg.insert(pm.occupant().canonical_key().to_owned(), sd.degrees);
    }

    fn abs_separation_deg(a: f64, b: f64) -> f64 {
        let a = normalize_deg_0_360(a);
        let b = normalize_deg_0_360(b);
        let mut d = (a - b).abs() % 360.0;
        if d > 180.0 {
            d = 360.0 - d;
        }
        d
    }

    let dom_aspects = aspects_from_dom(document);
    if dom_aspects.is_empty() {
        out.push_str("  - (no rendered aspects)\n\n");
    } else {
        for (kind, a, b) in dom_aspects {
            let a_deg = endpoint_deg.get(&a).copied();
            let b_deg = endpoint_deg.get(&b).copied();

            match (a_deg, b_deg) {
                (Some(ad), Some(bd)) => {
                    let sep = abs_separation_deg(ad, bd);
                    out.push_str(&format!(
                        "  - {a} {kind} {b}: a={}, b={}, sep={:.4}°\n",
                        fmt_sign_deg(ad),
                        fmt_sign_deg(bd),
                        sep
                    ));
                }
                _ => {
                    // If we can't resolve one/both endpoints, still keep the edge line.
                    out.push_str(&format!("  - {a} {kind} {b}\n"));
                }
            }
        }
        out.push_str("\n");
    }

    Ok(out)
}

fn render_chart(
    theme_toml: &str,
    layout_toml: &str,
    data_toml: &str,
    aspects_toml: &str,
) -> Result<String, String> {
    let theme_file: ThemeFile =
        toml::from_str(theme_toml).map_err(|e| format!("theme TOML parse error: {e}"))?;

    let layout_file: LayoutFile =
        toml::from_str(layout_toml).map_err(|e| format!("layout TOML parse error: {e}"))?;

    let data_file: DataFile =
        toml::from_str(data_toml).map_err(|e| format!("data TOML parse error: {e}"))?;

    let theme = theme_file.theme;
    let layout = layout_file.layout;
    let data = data_file.data;

    // Basic debug diagnostics: surface what the renderer thinks is in the dataset.
    if let Some(ds) = data.dataset_bodies("natal") {
        web_sys::console::log_1(
            &format!(
                "rubrum_editor wasm: dataset 'natal' bodies count={} (includes chart points/angles/lots in this file)",
                ds.len()
            )
            .into(),
        );

        let mut counts = std::collections::BTreeMap::<&'static str, usize>::new();
        for pm in ds {
            let key = match pm.occupant() {
                rubrum_render::Occupant::Body(_) => "Body",
                rubrum_render::Occupant::ChartPoint(_) => "ChartPoint",
                rubrum_render::Occupant::Angle(_) => "Angle",
                rubrum_render::Occupant::Lot(_) => "Lot",
                rubrum_render::Occupant::Empty => "Empty",
            };
            *counts.entry(key).or_insert(0) += 1;
        }
        web_sys::console::log_1(
            &format!("rubrum_editor wasm: dataset occupant counts: {:?}", counts).into(),
        );
    } else {
        web_sys::console::error_1(&"rubrum_editor wasm: dataset 'natal' not found".into());
    }

    let aspects_file: AspectsFile =
        toml::from_str(aspects_toml).map_err(|e| format!("aspects TOML parse error: {e}"))?;

    let svg =
        rubrum_svg::chart_to_svg_string_spec(&theme, &layout, Some(&aspects_file.rules), &data)
            .map_err(|e| format!("render error: {e}"))?;

    web_sys::console::log_1(
        &format!(
            "rubrum_editor wasm: svg stats: len={}, uses={}, texts={}",
            svg.len(),
            svg.matches("<use ").count(),
            svg.matches("<text ").count()
        )
        .into(),
    );

    // Debug: emit a snippet of the first placement <use> so we can inspect attributes.
    // (This helps diagnose browser rendering issues like rotation/clipping.)
    if let Some(idx) = svg.find("class=\"rb-occupant rb-occupant-glyph\"") {
        let start = idx.saturating_sub(180);
        let end = (idx + 420).min(svg.len());
        let snippet = &svg[start..end];
        web_sys::console::log_1(
            &format!(
                "rubrum_editor wasm: first placement use snippet:\n{}",
                snippet
            )
            .into(),
        );
    }

    if svg.trim().is_empty() {
        return Err("renderer returned an empty SVG string".to_owned());
    }

    Ok(svg)
}

fn set_hidden(document: &web_sys::Document, id: &str, hidden: bool) {
    if let Some(el) = document.get_element_by_id(id) {
        if hidden {
            let _ = el.class_list().add_1("hidden");
        } else {
            let _ = el.class_list().remove_1("hidden");
        }
    }
}

fn parse_f64_attr(attrs: &std::collections::BTreeMap<String, String>, key: &str) -> Option<f64> {
    attrs.get(key).and_then(|v| v.parse::<f64>().ok())
}

fn sign_from_label(s: &str) -> Option<Sign> {
    // TOML examples use Rust enum variant strings (e.g. "Aries").
    match s {
        "Aries" => Some(Sign::Aries),
        "Taurus" => Some(Sign::Taurus),
        "Gemini" => Some(Sign::Gemini),
        "Cancer" => Some(Sign::Cancer),
        "Leo" => Some(Sign::Leo),
        "Virgo" => Some(Sign::Virgo),
        "Libra" => Some(Sign::Libra),
        "Scorpio" => Some(Sign::Scorpio),
        "Sagittarius" => Some(Sign::Sagittarius),
        "Capricorn" => Some(Sign::Capricorn),
        "Aquarius" => Some(Sign::Aquarius),
        "Pisces" => Some(Sign::Pisces),
        _ => None,
    }
}

fn normalize_deg_0_360(deg: f64) -> f64 {
    let mut d = deg % 360.0;
    if d < 0.0 {
        d += 360.0;
    }
    d
}

fn parse_f64_from_attr(el: &Element, key: &str) -> Option<f64> {
    el.get_attribute(key).and_then(|v| v.parse::<f64>().ok())
}

fn svg_view_box(svg: &SvgsvgElement) -> Option<(f64, f64, f64, f64)> {
    let vb = svg.get_attribute("viewBox")?;
    let parts: Vec<f64> = vb
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() == 4 {
        Some((parts[0], parts[1], parts[2], parts[3]))
    } else {
        None
    }
}

fn svg_chart_meta(svg: &SvgsvgElement) -> Option<(f64, f64, f64)> {
    let el: Element = svg.clone().dyn_into().ok()?;
    let cx = parse_f64_from_attr(&el, "data-rb-cx")?;
    let cy = parse_f64_from_attr(&el, "data-rb-cy")?;
    let rotation_deg = parse_f64_from_attr(&el, "data-rb-rotation-deg").unwrap_or(0.0);
    Some((cx, cy, rotation_deg))
}

fn client_to_svg_xy(svg: &SvgsvgElement, client_x: f64, client_y: f64) -> Option<(f64, f64)> {
    let rect = svg.get_bounding_client_rect();
    let rect_w = rect.width();
    let rect_h = rect.height();

    if rect_w <= 0.0 || rect_h <= 0.0 {
        return None;
    }

    let (vb_x, vb_y, vb_w, vb_h) = svg_view_box(svg).unwrap_or((0.0, 0.0, rect_w, rect_h));

    let x = (client_x - rect.left()) * (vb_w / rect_w) + vb_x;
    let y = (client_y - rect.top()) * (vb_h / rect_h) + vb_y;

    Some((x, y))
}

fn svg_xy_to_lon_deg(cx: f64, cy: f64, x: f64, y: f64) -> f64 {
    // SVG coordinates have Y increasing downward. The chart coordinate convention used by
    // `polar_to_xy(...)` expects Y to be flipped so +deg rotates counter-clockwise.
    let dx = x - cx;
    let dy = cy - y;
    normalize_deg_0_360(dy.atan2(dx).to_degrees())
}

fn signed_angle_delta_deg(prev_deg: f64, next_deg: f64) -> f64 {
    // Smallest signed delta to go from prev → next, in (-180, 180].
    let mut d = (next_deg - prev_deg) % 360.0;
    if d <= -180.0 {
        d += 360.0;
    }
    if d > 180.0 {
        d -= 360.0;
    }
    d
}

fn update_placement_dom_for_display_deg(
    placement_el: &Element,
    hit_el: &Element,
    glyph_el: Option<&Element>,
    cx: f64,
    cy: f64,
    r: f64,
    display_deg: f64,
    abs_deg: f64,
) {
    let display_deg = normalize_deg_0_360(display_deg);
    let (x, y) = polar_to_xy(cx, cy, r, display_deg);

    set_attr_f64(hit_el, "cx", x);
    set_attr_f64(hit_el, "cy", y);

    if let Some(glyph_el) = glyph_el {
        update_glyph_center_xy(glyph_el, x, y);
    }

    set_attr_f64(placement_el, "data-rb-degree", normalize_deg_0_360(abs_deg));
}

fn chart_svg_root(document: &web_sys::Document) -> Option<SvgsvgElement> {
    let chart = document.get_element_by_id("chart")?;
    let svg_el = chart.query_selector("svg").ok().flatten()?;
    svg_el.dyn_into::<SvgsvgElement>().ok()
}

#[allow(dead_code)]
fn placement_attrs_from_el(el: &Element) -> std::collections::BTreeMap<String, String> {
    let mut attrs = std::collections::BTreeMap::<String, String>::new();
    for key in [
        "data-rb-dataset",
        "data-rb-endpoint",
        "data-rb-occupant",
        "data-rb-occupant-type",
        "data-rb-degree",
        "data-rb-retrograde",
    ] {
        if let Some(v) = el.get_attribute(key) {
            attrs.insert(key.to_owned(), v);
        }
    }
    attrs
}

fn placement_dom_from_target(
    target: web_sys::EventTarget,
) -> Option<(Element, Element, Option<Element>)> {
    let start_el: Element = target.dyn_into().ok()?;
    let mut el = start_el.clone();

    // Walk up until we reach the placement <g> wrapper.
    loop {
        if el.class_list().contains("rb-placement") {
            break;
        }
        el = el.parent_element()?;
    }

    let placement_el = el;

    // Prefer the actual hit circle if the pointer-down target is it.
    let hit_el = if start_el.class_list().contains("rb-placement-hit") {
        start_el
    } else {
        placement_el
            .query_selector(".rb-placement-hit")
            .ok()
            .flatten()?
    };

    let glyph_el = placement_el
        .query_selector("use")
        .ok()
        .flatten()
        .or_else(|| placement_el.query_selector("text").ok().flatten());

    Some((placement_el, hit_el, glyph_el))
}

fn find_placement_dom_by_key(
    svg_root: &Element,
    dataset: &str,
    endpoint: &str,
) -> Option<(Element, Element, Option<Element>)> {
    let list = svg_root.query_selector_all(".rb-placement").ok()?;
    let len = list.length();

    for i in 0..len {
        let Some(node) = list.item(i) else {
            continue;
        };
        let Ok(el) = node.dyn_into::<Element>() else {
            continue;
        };

        let ds = el.get_attribute("data-rb-dataset").unwrap_or_default();
        if ds != dataset {
            continue;
        }

        let ep = el
            .get_attribute("data-rb-endpoint")
            .or_else(|| el.get_attribute("data-rb-occupant"))
            .unwrap_or_default();

        if ep != endpoint {
            continue;
        }

        let hit_el = el.query_selector(".rb-placement-hit").ok().flatten()?;
        let glyph_el = el
            .query_selector("use")
            .ok()
            .flatten()
            .or_else(|| el.query_selector("text").ok().flatten());

        return Some((el, hit_el, glyph_el));
    }

    None
}

fn hit_circle_center_xy(hit_el: &Element) -> Option<(f64, f64)> {
    let x = parse_f64_from_attr(hit_el, "cx")?;
    let y = parse_f64_from_attr(hit_el, "cy")?;
    Some((x, y))
}

fn set_attr_f64(el: &Element, key: &str, v: f64) {
    let _ = el.set_attribute(key, format_float_compact(v).as_str());
}

fn update_glyph_center_xy(glyph_el: &Element, x: f64, y: f64) {
    let tag = glyph_el.tag_name().to_ascii_lowercase();
    match tag.as_str() {
        "text" => {
            set_attr_f64(glyph_el, "x", x);
            set_attr_f64(glyph_el, "y", y);
        }
        "use" => {
            let size = parse_f64_from_attr(glyph_el, "width")
                .or_else(|| parse_f64_from_attr(glyph_el, "height"))
                .unwrap_or(0.0);

            // The pure-SVG renderer positions sprite glyphs via `transform="translate(x0 y0)"`
            // (instead of `x/y`) for better cross-browser behavior. Keep that convention here so
            // in-place dragging/scrubbing moves glyphs correctly.
            if size > 0.0 {
                let x0 = x - (size / 2.0);
                let y0 = y - (size / 2.0);

                let _ = glyph_el.set_attribute(
                    "transform",
                    format!(
                        "translate({} {})",
                        format_float_compact(x0),
                        format_float_compact(y0)
                    )
                    .as_str(),
                );

                // Avoid updating `x/y` because it can interact poorly with external sprites.
                let _ = glyph_el.remove_attribute("x");
                let _ = glyph_el.remove_attribute("y");
            } else {
                // Fallback: keep the existing offset and just move the anchor.
                set_attr_f64(glyph_el, "x", x);
                set_attr_f64(glyph_el, "y", y);
            }
        }
        _ => {}
    }
}

fn sync_placement_editor_for_degree(document: &web_sys::Document, deg: f64) {
    let d = normalize_deg_0_360(deg);
    let (sign, in_sign) = degrees_to_sign_parts(d);

    set_input_value(document, "placement_deg", d);
    set_input_value_string(
        document,
        "placement_deg_slider",
        format_float_compact(d).as_str(),
    );
    set_select_value(document, "placement_sign", format!("{sign:?}").as_str());
    set_input_value(document, "placement_sign_deg", in_sign);
}

fn degrees_to_sign_parts(deg: f64) -> (Sign, f64) {
    let d = normalize_deg_0_360(deg);

    let sign_idx = (d / 30.0).floor() as i32;
    let sign = match sign_idx {
        0 => Sign::Aries,
        1 => Sign::Taurus,
        2 => Sign::Gemini,
        3 => Sign::Cancer,
        4 => Sign::Leo,
        5 => Sign::Virgo,
        6 => Sign::Libra,
        7 => Sign::Scorpio,
        8 => Sign::Sagittarius,
        9 => Sign::Capricorn,
        10 => Sign::Aquarius,
        _ => Sign::Pisces,
    };

    let in_sign = d - sign.start_degree_f64();
    (sign, in_sign)
}

fn populate_sign_select(document: &web_sys::Document) {
    let Some(el) = document.get_element_by_id("placement_sign") else {
        return;
    };

    let Ok(select) = el.dyn_into::<HtmlSelectElement>() else {
        return;
    };

    if select.length() > 0 {
        return;
    }

    let mut html = String::new();
    for (label, value) in [
        ("Aries", "Aries"),
        ("Taurus", "Taurus"),
        ("Gemini", "Gemini"),
        ("Cancer", "Cancer"),
        ("Leo", "Leo"),
        ("Virgo", "Virgo"),
        ("Libra", "Libra"),
        ("Scorpio", "Scorpio"),
        ("Sagittarius", "Sagittarius"),
        ("Capricorn", "Capricorn"),
        ("Aquarius", "Aquarius"),
        ("Pisces", "Pisces"),
    ] {
        html.push_str(&format!("<option value=\"{value}\">{label}</option>"));
    }

    select.set_inner_html(&html);
}

fn get_input_f64(document: &web_sys::Document, id: &str) -> Option<f64> {
    let el = document.get_element_by_id(id)?;
    let input: HtmlInputElement = el.dyn_into().ok()?;
    input.value().parse::<f64>().ok()
}

fn set_input_value(document: &web_sys::Document, id: &str, value: f64) {
    if let Some(el) = document.get_element_by_id(id) {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value(&format!("{:.2}", value));
        }
    }
}

fn set_input_value_string(document: &web_sys::Document, id: &str, value: &str) {
    if let Some(el) = document.get_element_by_id(id) {
        if let Ok(input) = el.dyn_into::<HtmlInputElement>() {
            input.set_value(value);
        }
    }
}

fn format_float_compact(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }

    let mut s = format!("{:.6}", v);
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() { "0".to_owned() } else { s }
}

fn clamp_step_size_deg(step_deg: f64) -> f64 {
    if !step_deg.is_finite() {
        return 1.0;
    }

    step_deg.clamp(0.01, 30.0)
}

fn set_number_input_step(document: &web_sys::Document, id: &str, step_deg: f64) {
    let Some(el) = document.get_element_by_id(id) else {
        return;
    };

    let Ok(input) = el.dyn_into::<HtmlInputElement>() else {
        return;
    };

    input.set_step(format_float_compact(step_deg).as_str());
}

fn set_copy_status(document: &web_sys::Document, msg: &str) {
    set_text_by_id(document, "copy_llm_status", msg);
}

async fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    // Prefer the modern async clipboard API.
    //
    // We use `js_sys::Reflect` rather than web-sys Clipboard bindings to keep feature flags minimal.
    // Also avoid `window.navigator()` so we don't need additional `web-sys` feature flags.
    let global = js_sys::global();

    let navigator = Reflect::get(&global, &JsValue::from_str("navigator"))
        .map_err(|_| "navigator unavailable".to_owned())?;

    let clipboard = Reflect::get(&navigator, &JsValue::from_str("clipboard"))
        .map_err(|_| "navigator.clipboard unavailable".to_owned())?;

    if clipboard.is_null() || clipboard.is_undefined() {
        return Err("navigator.clipboard unavailable".to_owned());
    }

    let write_text = Reflect::get(&clipboard, &JsValue::from_str("writeText"))
        .map_err(|_| "navigator.clipboard.writeText unavailable".to_owned())?
        .dyn_into::<Function>()
        .map_err(|_| "navigator.clipboard.writeText is not a function".to_owned())?;

    let promise_js = write_text
        .call1(&clipboard, &JsValue::from_str(text))
        .map_err(|e| format!("navigator.clipboard.writeText threw: {e:?}"))?;

    let promise = promise_js
        .dyn_into::<Promise>()
        .map_err(|_| "navigator.clipboard.writeText did not return a Promise".to_owned())?;

    JsFuture::from(promise)
        .await
        .map_err(|e| format!("clipboard Promise rejected: {e:?}"))?;

    Ok(())
}

fn copy_textarea_via_exec_command(
    document: &web_sys::Document,
    textarea_id: &str,
) -> Result<(), String> {
    // Fallback for older browsers / insecure contexts: select the textarea then call
    // `document.execCommand("copy")`.
    let el = document
        .get_element_by_id(textarea_id)
        .ok_or_else(|| format!("missing #{textarea_id} element"))?;

    let ta: web_sys::HtmlTextAreaElement = el
        .dyn_into()
        .map_err(|_| format!("#{textarea_id} element is not a <textarea>"))?;

    // Best effort: focus + select.
    let _ = ta.focus();
    ta.select();

    let doc_js: JsValue = document.clone().into();

    let exec = Reflect::get(&doc_js, &JsValue::from_str("execCommand"))
        .map_err(|_| "document.execCommand unavailable".to_owned())?
        .dyn_into::<Function>()
        .map_err(|_| "document.execCommand is not a function".to_owned())?;

    let ok_js = exec
        .call1(&doc_js, &JsValue::from_str("copy"))
        .map_err(|e| format!("document.execCommand threw: {e:?}"))?;

    match ok_js.as_bool() {
        Some(true) => Ok(()),
        _ => Err("document.execCommand returned false".to_owned()),
    }
}

fn apply_step_size_to_placement_inputs(document: &web_sys::Document, step_deg: f64) {
    let step_deg = clamp_step_size_deg(step_deg);
    set_number_input_step(document, "placement_deg", step_deg);
    set_number_input_step(document, "placement_sign_deg", step_deg);
}

fn get_step_size_deg(document: &web_sys::Document) -> f64 {
    // Prefer the numeric input as the source of truth.
    let n = get_input_f64(document, "step_size_num")
        .or_else(|| get_input_f64(document, "step_size"))
        .unwrap_or(1.0);
    clamp_step_size_deg(n)
}

fn set_select_value(document: &web_sys::Document, id: &str, value: &str) {
    if let Some(el) = document.get_element_by_id(id) {
        if let Ok(select) = el.dyn_into::<HtmlSelectElement>() {
            select.set_value(value);
        }
    }
}

fn update_placement_editor_ui(document: &web_sys::Document, selection: Option<&Selection>) {
    populate_sign_select(document);

    let Some(sel) = selection else {
        set_hidden(document, "placement_editor", true);
        set_hidden(document, "placement_stepper", true);
        return;
    };

    if sel.kind != SelectionKind::Placement {
        set_hidden(document, "placement_editor", true);
        set_hidden(document, "placement_stepper", true);
        return;
    }

    let Some(deg) = parse_f64_attr(&sel.attrs, "data-rb-degree") else {
        set_hidden(document, "placement_editor", true);
        set_hidden(document, "placement_stepper", true);
        return;
    };

    let (sign, in_sign) = degrees_to_sign_parts(deg);

    set_input_value(document, "placement_deg", deg);
    set_input_value_string(
        document,
        "placement_deg_slider",
        format_float_compact(deg).as_str(),
    );
    set_select_value(document, "placement_sign", format!("{sign:?}").as_str());
    set_input_value(document, "placement_sign_deg", in_sign);

    // Step size controls apply to all placements (it controls the number-input arrow increment).
    set_hidden(document, "placement_stepper", false);

    // Ensure number-input arrows use the current step size.
    let step_deg = get_step_size_deg(document);
    apply_step_size_to_placement_inputs(document, step_deg);

    set_hidden(document, "placement_editor", false);
}

fn update_data_toml_selected_degree(
    data_toml: &str,
    dataset: &str,
    endpoint: &str,
    new_deg: f64,
) -> Result<String, String> {
    let mut data_file: DataFile =
        toml::from_str(data_toml).map_err(|e| format!("data TOML parse error: {e}"))?;

    let mut updated = false;

    for ds in data_file.data.datasets.iter_mut() {
        if ds.id != dataset {
            continue;
        }

        for pm in ds.bodies.iter_mut() {
            if pm.occupant().canonical_key() != endpoint {
                continue;
            }

            pm.placement.coordinate = Coordinate::SignDegree(SignDegree::new(new_deg));
            updated = true;
            break;
        }

        if updated {
            break;
        }
    }

    // Backwards-compat: allow editing the legacy `natal_bodies` list.
    if !updated && dataset == "natal" {
        for pm in data_file.data.natal_bodies.iter_mut() {
            if pm.occupant().canonical_key() != endpoint {
                continue;
            }

            pm.placement.coordinate = Coordinate::SignDegree(SignDegree::new(new_deg));
            updated = true;
            break;
        }
    }

    if !updated {
        return Err(format!(
            "Selected endpoint '{endpoint}' not found in dataset '{dataset}'"
        ));
    }

    toml::to_string_pretty(&data_file).map_err(|e| format!("data TOML serialize error: {e}"))
}

fn set_error(document: &web_sys::Document, msg: Option<&str>) {
    if let Some(el) = document.get_element_by_id("render_error") {
        match msg {
            Some(msg) => {
                el.set_inner_html(&escape_html(msg));
                let _ = el.class_list().remove_1("hidden");
            }
            None => {
                el.set_inner_html("");
                let _ = el.class_list().add_1("hidden");
            }
        }
    }
}

fn textarea_value(document: &web_sys::Document, id: &str) -> Result<String, String> {
    let el = document
        .get_element_by_id(id)
        .ok_or_else(|| format!("missing #{id} element"))?;

    let ta: web_sys::HtmlTextAreaElement = el
        .dyn_into()
        .map_err(|_| format!("#{id} element is not a <textarea>"))?;

    Ok(ta.value())
}

fn set_textarea_value(document: &web_sys::Document, id: &str, value: &str) -> Result<(), String> {
    let el = document
        .get_element_by_id(id)
        .ok_or_else(|| format!("missing #{id} element"))?;

    let ta: web_sys::HtmlTextAreaElement = el
        .dyn_into()
        .map_err(|_| format!("#{id} element is not a <textarea>"))?;

    // Preserve caret/selection where possible so programmatic TOML updates don't feel like the
    // cursor is jumping around.
    let was_focused = document
        .active_element()
        .and_then(|active| active.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
        .is_some_and(|active| active.id() == id);

    let selection_start = ta.selection_start().ok().flatten();
    let selection_end = ta.selection_end().ok().flatten();

    ta.set_value(value);

    if was_focused {
        if let (Some(start), Some(end)) = (selection_start, selection_end) {
            let len = value.chars().count() as u32;
            let start = start.min(len);
            let end = end.min(len);
            let _ = ta.set_selection_range(start, end);
        }
    }

    Ok(())
}

fn dispatch_input_event(document: &web_sys::Document, id: &str) {
    if let Some(el) = document.get_element_by_id(id) {
        if let Ok(evt) = web_sys::Event::new("input") {
            let _ = el.dispatch_event(&evt);
        }
    }
}

fn set_selection_text(document: &web_sys::Document, text: &str) {
    if let Some(el) = document.get_element_by_id("selection_info") {
        el.set_text_content(Some(text));
    }
}

fn set_text_by_id(document: &web_sys::Document, id: &str, text: &str) {
    if let Some(el) = document.get_element_by_id(id) {
        el.set_text_content(Some(text));
    }
}

fn element_is_in_chart_svg(document: &web_sys::Document, el: &Element) -> bool {
    let Some(svg) = chart_svg_root(document) else {
        return false;
    };

    let Ok(svg_node) = svg.dyn_into::<web_sys::Node>() else {
        return false;
    };

    let Ok(el_node) = el.clone().dyn_into::<web_sys::Node>() else {
        return false;
    };

    svg_node.contains(Some(&el_node))
}

fn set_svg_selected_class(document: &web_sys::Document, selection: &Selection, selected: bool) {
    // Only highlight elements that are part of the injected chart SVG.
    if !element_is_in_chart_svg(document, &selection.el) {
        return;
    }

    // We treat Chart selections as "no highlight" to avoid highlighting arbitrary wrapper DOM.
    if selection.kind == SelectionKind::Chart {
        return;
    }

    if selected {
        let _ = selection.el.class_list().add_1("rb-selected");
    } else {
        let _ = selection.el.class_list().remove_1("rb-selected");
    }
}

fn apply_svg_selection_highlight(
    document: &web_sys::Document,
    prev: Option<&Selection>,
    next: Option<&Selection>,
) {
    if let Some(prev) = prev {
        set_svg_selected_class(document, prev, false);
    }

    if let Some(next) = next {
        set_svg_selected_class(document, next, true);
    }
}

fn update_structure_panel_ui(document: &web_sys::Document, selection: Option<&Selection>) {
    let Some(sel) = selection else {
        set_hidden(document, "structure_panel", true);
        return;
    };

    if sel.kind != SelectionKind::Structure {
        set_hidden(document, "structure_panel", true);
        return;
    }

    set_hidden(document, "structure_panel", false);

    let kind = sel
        .attrs
        .get("data-rb-structure")
        .map(|s| s.as_str())
        .unwrap_or("—");

    let band = sel
        .attrs
        .get("data-rb-band")
        .map(|s| s.as_str())
        .unwrap_or("—");

    let lane = sel
        .attrs
        .get("data-rb-lane-id")
        .map(|s| s.as_str())
        .unwrap_or("—");

    let lane_index = sel
        .attrs
        .get("data-rb-lane-index")
        .map(|s| s.as_str())
        .unwrap_or("—");

    let axis = sel
        .attrs
        .get("data-rb-axis")
        .map(|s| s.as_str())
        .unwrap_or("—");

    let deg_raw = sel
        .attrs
        .get("data-rb-deg")
        .or_else(|| sel.attrs.get("data-rb-degree"));

    let deg = match deg_raw {
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .map(format_float_compact)
            .unwrap_or_else(|| raw.to_owned()),
        None => "—".to_owned(),
    };

    set_text_by_id(document, "structure_kind", kind);
    set_text_by_id(document, "structure_band", band);
    set_text_by_id(document, "structure_lane", lane);
    set_text_by_id(document, "structure_lane_index", lane_index);
    set_text_by_id(document, "structure_axis", axis);
    set_text_by_id(document, "structure_deg", deg.as_str());
}

fn selection_from_target(target: web_sys::EventTarget) -> Option<Selection> {
    let mut el: Element = target.dyn_into().ok()?;

    // Walk up the DOM tree, looking for stable rubrum_cairo metadata.
    loop {
        let mut attrs = std::collections::BTreeMap::<String, String>::new();

        for key in [
            // Placement selection metadata.
            "data-rb-dataset",
            "data-rb-endpoint",
            "data-rb-occupant",
            "data-rb-occupant-type",
            "data-rb-degree",
            "data-rb-retrograde",
            // Aspect selection metadata.
            "data-rb-aspect-kind",
            "data-rb-aspect-a",
            "data-rb-aspect-b",
            // Structure selection metadata (bands/lanes/etc.).
            "data-rb-structure",
            "data-rb-band",
            "data-rb-lane-id",
            "data-rb-lane-index",
            "data-rb-axis",
            "data-rb-deg",
            "data-rb-house-set",
            "data-rb-house",
            "data-rb-sign-index",
            "data-rb-r-inner",
            "data-rb-r-outer",
            // Chart-wide metadata (emitted on the root <svg>).
            "data-rb-cx",
            "data-rb-cy",
            "data-rb-rotation-deg",
        ] {
            if let Some(v) = el.get_attribute(key) {
                attrs.insert(key.to_owned(), v);
            }
        }

        let kind = if attrs.contains_key("data-rb-aspect-kind") {
            Some(SelectionKind::Aspect)
        } else if attrs.contains_key("data-rb-occupant") || attrs.contains_key("data-rb-endpoint") {
            Some(SelectionKind::Placement)
        } else if attrs.contains_key("data-rb-structure") {
            Some(SelectionKind::Structure)
        } else {
            None
        };

        if let Some(kind) = kind {
            return Some(Selection {
                kind,
                attrs,
                el: el.clone(),
            });
        }

        // Root element reached without stable metadata → treat as a chart-wide selection.
        let Some(parent) = el.parent_element() else {
            return Some(Selection {
                kind: SelectionKind::Chart,
                attrs,
                el,
            });
        };

        el = parent;
    }
}

fn render_aspect_toggles(document: &web_sys::Document, aspects_toml: &str) -> Result<(), String> {
    let Some(container) = document.get_element_by_id("aspect_toggles") else {
        return Ok(());
    };

    let aspects_file: AspectsFile =
        toml::from_str(aspects_toml).map_err(|e| format!("aspects TOML parse error: {e}"))?;

    // If rules are missing or empty, use defaults so the UI still shows something.
    let mut rules = aspects_file.rules;
    if rules.aspects.is_empty() {
        rules = Default::default();
    }

    let mut html = String::new();

    for rule in rules.aspects.iter() {
        let kind_name = rule.kind.to_string();
        let symbol = rule.kind.symbol_text();
        let checked = if rule.enabled { "checked" } else { "" };

        html.push_str(&format!(
            "<label class=\"toggle\">\\
<input type=\"checkbox\" data-rb-aspect-kind=\"{}\" {} />\\
<span><span class=\"kind\">{}</span><span class=\"symbol\">{}</span></span>\\
</label>",
            escape_html(kind_name.as_str()),
            checked,
            escape_html(kind_name.as_str()),
            escape_html(symbol)
        ));
    }

    container.set_inner_html(&html);
    Ok(())
}

fn update_aspects_toml_aspect_enabled(
    aspects_toml: &str,
    kind_name: &str,
    enabled: bool,
) -> Result<String, String> {
    let mut aspects_file: AspectsFile =
        toml::from_str(aspects_toml).map_err(|e| format!("aspects TOML parse error: {e}"))?;

    // If the rule list is empty, treat that as "use defaults", but we want explicit entries so
    // the toggles can mutate `enabled` flags.
    if aspects_file.rules.aspects.is_empty() {
        aspects_file.rules = Default::default();
    }

    if let Some(rule) = aspects_file
        .rules
        .aspects
        .iter_mut()
        .find(|r| r.kind.to_string() == kind_name)
    {
        rule.enabled = enabled;
    }

    toml::to_string_pretty(&aspects_file).map_err(|e| format!("aspects TOML serialize error: {e}"))
}

fn run() -> Result<(), JsValue> {
    // Better panic messages in the browser console.
    console_error_panic_hook::set_once();

    debug_log("rubrum_editor wasm: start");

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing document"))?;

    let mount = document
        .get_element_by_id("chart")
        .ok_or_else(|| JsValue::from_str("missing #chart element"))?;

    // Seed editors with embedded default TOML.
    //
    // These live in `rubrum_render` so they can be shared across frontends and so the WASM app
    // doesn't rely on local filesystem paths.
    let default_theme_toml = rubrum_render::embedded_configs::THEME_DARK_TOML;
    let default_layout_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_LAYOUT_ONLY_TOML;
    let default_data_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_DATA_TOML;
    let default_aspects_toml = rubrum_render::embedded_configs::CHART_SPEC_NATAL_ASPECTS_TOML;

    if let Some(el) = document.get_element_by_id("theme_toml") {
        if let Ok(ta) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
            if ta.value().trim().is_empty() {
                ta.set_value(default_theme_toml);
            }
        }
    }
    if let Some(el) = document.get_element_by_id("layout_toml") {
        if let Ok(ta) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
            if ta.value().trim().is_empty() {
                ta.set_value(default_layout_toml);
            }
        }
    }
    if let Some(el) = document.get_element_by_id("data_toml") {
        if let Ok(ta) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
            if ta.value().trim().is_empty() {
                ta.set_value(default_data_toml);
            }
        }
    }

    if let Some(el) = document.get_element_by_id("aspects_toml") {
        if let Ok(ta) = el.dyn_into::<web_sys::HtmlTextAreaElement>() {
            if ta.value().trim().is_empty() {
                ta.set_value(default_aspects_toml);
            }
        }
    }

    // Setup Prism syntax highlighting for the TOML editors + export view.
    //
    // We do this after seeding the default textareas so the initial highlight overlay matches.
    //
    // IMPORTANT: Always initialize the highlight overlay, even if Prism isn't ready yet.
    // - If Prism is missing/unready, we fall back to a plain-text escaped render in the <pre><code>
    //   layer (see `prism_highlight_element`).
    // - Once Prism + language shims become ready, we retry and re-highlight.
    setup_prism_highlighting(&document);

    // Properties panel: bind event handlers and seed values from the current theme TOML.
    setup_properties_panel(&document);

    // Advanced drawer: bind event handlers for open/close and tab switching.
    setup_advanced_drawer(&document);

    // Keep properties UI in sync when the user edits theme TOML directly.
    if let Some(theme_el) = document.get_element_by_id("theme_toml") {
        let document_for_prop_sync = document.clone();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            sync_properties_from_theme_toml(&document_for_prop_sync);
        });

        let _ = theme_el.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Trunk injects the WASM loader near the rust <link> tag; depending on script load timing,
    // Prism (and its language shims) might not be ready when the WASM start hook runs. Retry
    // briefly until Prism is ready so we can switch from plain text to tokenized highlighting.
    if !prism_ready() {
        schedule_prism_setup_retry(window.clone(), document.clone(), 0);
    }

    // Render *something* immediately so we can distinguish "code didn't run" from "render failed".
    mount.set_inner_html("<div class=\"loading\">WASM started…</div>");

    let selection_state = std::rc::Rc::new(std::cell::RefCell::new(None::<Selection>));

    // Pointer-driven placement dragging state.
    let drag_state = std::rc::Rc::new(std::cell::RefCell::new(None::<DragState>));
    let suppress_next_click = std::rc::Rc::new(std::cell::RefCell::new(false));

    // Pointer-driven scrubbing on the degree slider (supports wrap-around).
    let slider_drag_state = std::rc::Rc::new(std::cell::RefCell::new(None::<SliderDragState>));
    let suppress_slider_input = std::rc::Rc::new(std::cell::RefCell::new(false));

    let do_render = {
        let document = document.clone();
        let mount = mount.clone();

        move || {
            let theme_toml = textarea_value(&document, "theme_toml")
                .unwrap_or_else(|_| default_theme_toml.to_owned());
            let layout_toml = textarea_value(&document, "layout_toml")
                .unwrap_or_else(|_| default_layout_toml.to_owned());
            let data_toml = textarea_value(&document, "data_toml")
                .unwrap_or_else(|_| default_data_toml.to_owned());
            let aspects_toml = textarea_value(&document, "aspects_toml")
                .unwrap_or_else(|_| default_aspects_toml.to_owned());

            match render_chart(&theme_toml, &layout_toml, &data_toml, &aspects_toml) {
                Ok(svg) => {
                    set_error(&document, None);
                    mount.set_inner_html(&svg);

                    // Populate the LLM export textarea after each successful render.
                    //
                    // This runs after we inject the SVG into the DOM so we can read the currently
                    // rendered aspects from `#rb-aspects`.
                    {
                        let export_text = match build_llm_export_text(
                            &theme_toml,
                            &layout_toml,
                            &data_toml,
                            &document,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                format!(
                                    "# Chart export (LLM-friendly)\n\n(export unavailable: {e})\n"
                                )
                            }
                        };

                        let _ = set_textarea_value(&document, "llm_export", export_text.as_str());
                        set_code_highlight(&document, "llm_export_highlight", export_text.as_str());
                        set_copy_status(&document, "");
                    }

                    // Keep the aspect toggles in sync with the current aspects rules TOML.
                    if let Err(e) = render_aspect_toggles(&document, &aspects_toml) {
                        web_sys::console::error_1(&e.into());
                    }

                    // Post-injection sanity check: make sure we actually have an <svg> element.
                    match mount.query_selector("svg") {
                        Ok(Some(svg_el)) => {
                            web_sys::console::log_1(&"rubrum_cairo wasm: svg element found".into());

                            // Log a few attributes to help diagnose zero-sized / invisible SVG.
                            if let Some(view_box) = svg_el.get_attribute("viewBox") {
                                web_sys::console::log_1(
                                    &format!("rubrum_cairo wasm: svg viewBox={view_box}").into(),
                                );
                            }
                            if let Some(width) = svg_el.get_attribute("width") {
                                web_sys::console::log_1(
                                    &format!("rubrum_cairo wasm: svg width={width}").into(),
                                );
                            }
                            if let Some(height) = svg_el.get_attribute("height") {
                                web_sys::console::log_1(
                                    &format!("rubrum_cairo wasm: svg height={height}").into(),
                                );
                            }
                        }
                        Ok(None) => {
                            let msg = "Rendered HTML did not contain an <svg> element";
                            web_sys::console::error_1(&msg.into());
                            set_error(&document, Some(msg));
                        }
                        Err(e) => {
                            web_sys::console::error_1(&e);
                        }
                    }
                }
                Err(msg) => {
                    web_sys::console::error_1(&msg.clone().into());
                    set_error(&document, Some(&msg));
                    mount.set_inner_html(
                        "<div class=\"loading\">Fix TOML errors to re-render…</div>",
                    );
                }
            }
        }
    };

    // Initial render.
    do_render();

    // LLM export: copy button.
    if let Some(btn_el) = document.get_element_by_id("copy_llm_export") {
        let document_for_copy = document.clone();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_evt: web_sys::Event| {
            let text = match textarea_value(&document_for_copy, "llm_export") {
                Ok(v) => v,
                Err(msg) => {
                    set_copy_status(&document_for_copy, format!("Copy failed: {msg}").as_str());
                    return;
                }
            };

            set_copy_status(&document_for_copy, "Copying...");

            // Prefer async clipboard; fall back to execCommand for insecure contexts/older browsers.
            let document_async = document_for_copy.clone();
            spawn_local(async move {
                match copy_text_to_clipboard(&text).await {
                    Ok(()) => {
                        set_copy_status(&document_async, "Copied.");
                    }
                    Err(err) => match copy_textarea_via_exec_command(&document_async, "llm_export")
                    {
                        Ok(()) => {
                            set_copy_status(&document_async, "Copied.");
                        }
                        Err(fallback) => {
                            set_copy_status(
                                &document_async,
                                format!("Copy failed: {err}; fallback failed: {fallback}").as_str(),
                            );
                        }
                    },
                }
            });
        });

        let _ = btn_el.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    // Pointer-drag placements to update their degrees.
    if let Some(chart_el) = document.get_element_by_id("chart") {
        let document_for_down = document.clone();
        let drag_state = drag_state.clone();
        let selection_state = selection_state.clone();

        let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |evt: PointerEvent| {
            // Only drag with the primary button.
            if evt.button() != 0 {
                return;
            }

            let Some(target) = evt.target() else {
                return;
            };

            let Some((placement_el, hit_el, glyph_el)) = placement_dom_from_target(target) else {
                return;
            };

            let Some(svg) = chart_svg_root(&document_for_down) else {
                return;
            };

            let Some((cx, cy, rotation_deg)) = svg_chart_meta(&svg) else {
                return;
            };

            let dataset = placement_el
                .get_attribute("data-rb-dataset")
                .unwrap_or_else(|| "natal".to_owned());
            let endpoint = placement_el
                .get_attribute("data-rb-endpoint")
                .or_else(|| placement_el.get_attribute("data-rb-occupant"))
                .unwrap_or_default();

            if endpoint.is_empty() {
                return;
            }

            let pointer_id = evt.pointer_id();
            let _ = hit_el.set_pointer_capture(pointer_id);

            let Some((px, py)) =
                client_to_svg_xy(&svg, evt.client_x() as f64, evt.client_y() as f64)
            else {
                return;
            };

            let pointer_display_deg = svg_xy_to_lon_deg(cx, cy, px, py);

            let Some((hit_x, hit_y)) = hit_circle_center_xy(&hit_el) else {
                return;
            };

            let center_display_deg = svg_xy_to_lon_deg(cx, cy, hit_x, hit_y);
            let offset_display_deg =
                signed_angle_delta_deg(pointer_display_deg, center_display_deg);

            let dx = hit_x - cx;
            let dy = hit_y - cy;
            let r = (dx * dx + dy * dy).sqrt();

            // Ensure the inspector + editor select the placement we are dragging.
            let mut attrs = placement_attrs_from_el(&placement_el);
            let abs_deg = parse_f64_from_attr(&placement_el, "data-rb-degree")
                .unwrap_or_else(|| normalize_deg_0_360(center_display_deg - rotation_deg));
            attrs.insert("data-rb-degree".to_owned(), format!("{:.2}", abs_deg));
            let sel = Selection {
                kind: SelectionKind::Placement,
                attrs,
                el: placement_el.clone(),
            };
            let summary = selection_summary(&sel);
            set_selection_text(&document_for_down, summary.as_str());
            update_placement_editor_ui(&document_for_down, Some(&sel));

            let prev = selection_state.borrow().clone();
            apply_svg_selection_highlight(&document_for_down, prev.as_ref(), Some(&sel));
            *selection_state.borrow_mut() = Some(sel);

            *drag_state.borrow_mut() = Some(DragState {
                pointer_id,
                dataset,
                endpoint,
                svg,
                cx,
                cy,
                rotation_deg,
                r,
                last_pointer_display_deg: pointer_display_deg,
                pointer_display_deg_unwrapped: pointer_display_deg,
                start_pointer_display_deg_unwrapped: pointer_display_deg,
                offset_display_deg,
                did_move: false,
                placement_el,
                hit_el,
                glyph_el,
                degree: abs_deg,
            });

            evt.prevent_default();
        });

        chart_el.add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    if let Some(chart_el) = document.get_element_by_id("chart") {
        let document_for_move = document.clone();
        let drag_state = drag_state.clone();
        let selection_state = selection_state.clone();

        let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |evt: PointerEvent| {
            let mut state = drag_state.borrow_mut();
            let Some(drag) = state.as_mut() else {
                return;
            };

            if evt.pointer_id() != drag.pointer_id {
                return;
            }

            let Some((px, py)) =
                client_to_svg_xy(&drag.svg, evt.client_x() as f64, evt.client_y() as f64)
            else {
                return;
            };

            let pointer_display_deg = svg_xy_to_lon_deg(drag.cx, drag.cy, px, py);
            let delta = signed_angle_delta_deg(drag.last_pointer_display_deg, pointer_display_deg);

            drag.pointer_display_deg_unwrapped += delta;
            drag.last_pointer_display_deg = pointer_display_deg;

            let center_display_deg_unwrapped =
                drag.pointer_display_deg_unwrapped + drag.offset_display_deg;
            let abs_deg = normalize_deg_0_360(center_display_deg_unwrapped - drag.rotation_deg);

            update_placement_dom_for_display_deg(
                &drag.placement_el,
                &drag.hit_el,
                drag.glyph_el.as_ref(),
                drag.cx,
                drag.cy,
                drag.r,
                center_display_deg_unwrapped,
                abs_deg,
            );

            drag.degree = abs_deg;

            if (drag.pointer_display_deg_unwrapped - drag.start_pointer_display_deg_unwrapped).abs()
                > 0.25
            {
                drag.did_move = true;
            }

            // Keep the inspector/editor in sync while dragging.
            {
                let mut state = selection_state.borrow_mut();
                if let Some(sel) = state.as_mut() {
                    let dataset = parse_dataset_attr(&sel.attrs);
                    let endpoint = parse_endpoint_attr(&sel.attrs);

                    if dataset.as_deref() == Some(drag.dataset.as_str())
                        && endpoint.as_deref() == Some(drag.endpoint.as_str())
                    {
                        sel.attrs
                            .insert("data-rb-degree".to_owned(), format!("{:.2}", abs_deg));
                        let summary = selection_summary(sel);
                        set_selection_text(&document_for_move, summary.as_str());
                        sync_placement_editor_for_degree(&document_for_move, abs_deg);
                    }
                }
            }

            evt.prevent_default();
        });

        chart_el.add_event_listener_with_callback("pointermove", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    if let Some(chart_el) = document.get_element_by_id("chart") {
        let document_for_up = document.clone();
        let drag_state = drag_state.clone();
        let suppress_next_click = suppress_next_click.clone();

        let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |evt: PointerEvent| {
            let finished = {
                let state = drag_state.borrow();
                state
                    .as_ref()
                    .is_some_and(|d| d.pointer_id == evt.pointer_id())
            };

            if !finished {
                return;
            }

            let drag = drag_state.borrow_mut().take();
            let Some(drag) = drag else {
                return;
            };

            if drag.did_move {
                *suppress_next_click.borrow_mut() = true;
            }

            let data_toml = match textarea_value(&document_for_up, "data_toml") {
                Ok(v) => v,
                Err(_) => return,
            };

            match update_data_toml_selected_degree(
                &data_toml,
                drag.dataset.as_str(),
                drag.endpoint.as_str(),
                drag.degree,
            ) {
                Ok(new_toml) => {
                    let _ = set_textarea_value(&document_for_up, "data_toml", new_toml.as_str());
                    dispatch_input_event(&document_for_up, "data_toml");
                }
                Err(msg) => {
                    set_error(&document_for_up, Some(msg.as_str()));
                }
            }

            evt.prevent_default();
        });

        chart_el.add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref())?;
        chart_el.add_event_listener_with_callback("pointercancel", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Pointer-drag the ecliptic-degree slider to scrub continuously (supports wrap-around).
    if let Some(slider_el) = document.get_element_by_id("placement_deg_slider") {
        if let Ok(slider_input) = slider_el.dyn_into::<HtmlInputElement>() {
            // pointerdown: capture + seed SliderDragState.
            {
                let document_for_down = document.clone();
                let selection_state = selection_state.clone();
                let slider_drag_state = slider_drag_state.clone();
                let suppress_slider_input = suppress_slider_input.clone();

                let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |evt: PointerEvent| {
                    if evt.button() != 0 {
                        return;
                    }

                    let slider_el = match evt.current_target() {
                        Some(t) => t,
                        None => return,
                    };
                    let Ok(slider_input) = slider_el.dyn_into::<HtmlInputElement>() else {
                        return;
                    };

                    // Only scrub when a placement is selected.
                    let Some(sel) = selection_state.borrow().clone() else {
                        return;
                    };
                    if sel.kind != SelectionKind::Placement {
                        return;
                    }

                    let Some(dataset) = parse_dataset_attr(&sel.attrs) else {
                        return;
                    };
                    let Some(endpoint) = parse_endpoint_attr(&sel.attrs) else {
                        return;
                    };

                    let Some(svg) = chart_svg_root(&document_for_down) else {
                        return;
                    };
                    let Some((cx, cy, rotation_deg)) = svg_chart_meta(&svg) else {
                        return;
                    };

                    let svg_root: Element = match svg.clone().dyn_into() {
                        Ok(v) => v,
                        Err(_) => return,
                    };

                    let Some((placement_el, hit_el, glyph_el)) =
                        find_placement_dom_by_key(&svg_root, dataset.as_str(), endpoint.as_str())
                    else {
                        return;
                    };

                    let Some((hit_x, hit_y)) = hit_circle_center_xy(&hit_el) else {
                        return;
                    };
                    let dx = hit_x - cx;
                    let dy = hit_y - cy;
                    let r = (dx * dx + dy * dy).sqrt();

                    // Slider pointer capture.
                    let pointer_id = evt.pointer_id();
                    let _ = slider_input.set_pointer_capture(pointer_id);

                    let rect = slider_input.get_bounding_client_rect();
                    let slider_width_px = rect.width().max(1.0);
                    let start_client_x = evt.client_x() as f64;

                    // Seed from the current selection's degree if possible.
                    let start_deg = sel
                        .attrs
                        .get("data-rb-degree")
                        .and_then(|v| v.parse::<f64>().ok())
                        .or_else(|| slider_input.value().parse::<f64>().ok())
                        .unwrap_or(0.0);

                    *suppress_slider_input.borrow_mut() = true;

                    *slider_drag_state.borrow_mut() = Some(SliderDragState {
                        pointer_id,
                        dataset,
                        endpoint,
                        svg,
                        cx,
                        cy,
                        rotation_deg,
                        r,
                        placement_el,
                        hit_el,
                        glyph_el,
                        start_client_x,
                        slider_width_px,
                        start_deg: normalize_deg_0_360(start_deg),
                        degree: normalize_deg_0_360(start_deg),
                    });

                    evt.prevent_default();
                });

                slider_input
                    .add_event_listener_with_callback("pointerdown", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }

            // pointermove: update UI + DOM live.
            {
                let document_for_move = document.clone();
                let selection_state = selection_state.clone();
                let slider_drag_state = slider_drag_state.clone();

                let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |evt: PointerEvent| {
                    let mut state = slider_drag_state.borrow_mut();
                    let Some(drag) = state.as_mut() else {
                        return;
                    };

                    if evt.pointer_id() != drag.pointer_id {
                        return;
                    }

                    let slider_el = match evt.current_target() {
                        Some(t) => t,
                        None => return,
                    };
                    let Ok(slider_input) = slider_el.dyn_into::<HtmlInputElement>() else {
                        return;
                    };

                    let dx_px = (evt.client_x() as f64) - drag.start_client_x;
                    let delta_deg = (dx_px / drag.slider_width_px) * 360.0;
                    let abs_deg = normalize_deg_0_360(drag.start_deg + delta_deg);

                    // Update slider + editor fields.
                    slider_input.set_value(format_float_compact(abs_deg).as_str());
                    sync_placement_editor_for_degree(&document_for_move, abs_deg);

                    // Update SVG placement in-place.
                    //
                    // Use the cached <svg> element so if the viewBox/meta changes (e.g. due to a
                    // re-render), we still compute display coordinates consistently.
                    if let Some((cx, cy, rotation_deg)) = svg_chart_meta(&drag.svg) {
                        drag.cx = cx;
                        drag.cy = cy;
                        drag.rotation_deg = rotation_deg;
                    }

                    let display_deg = normalize_deg_0_360(abs_deg + drag.rotation_deg);
                    update_placement_dom_for_display_deg(
                        &drag.placement_el,
                        &drag.hit_el,
                        drag.glyph_el.as_ref(),
                        drag.cx,
                        drag.cy,
                        drag.r,
                        display_deg,
                        abs_deg,
                    );

                    drag.degree = abs_deg;

                    // Keep the inspector selection metadata in sync.
                    {
                        let mut state = selection_state.borrow_mut();
                        if let Some(sel) = state.as_mut() {
                            let dataset = parse_dataset_attr(&sel.attrs);
                            let endpoint = parse_endpoint_attr(&sel.attrs);
                            if dataset.as_deref() == Some(drag.dataset.as_str())
                                && endpoint.as_deref() == Some(drag.endpoint.as_str())
                            {
                                sel.attrs
                                    .insert("data-rb-degree".to_owned(), format!("{:.2}", abs_deg));
                                let summary = selection_summary(sel);
                                set_selection_text(&document_for_move, summary.as_str());
                            }
                        }
                    }

                    evt.prevent_default();
                });

                slider_input
                    .add_event_listener_with_callback("pointermove", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }

            // pointerup/cancel: commit to TOML + allow normal input events again.
            {
                let document_for_up = document.clone();
                let slider_drag_state = slider_drag_state.clone();
                let suppress_slider_input = suppress_slider_input.clone();

                let cb = Closure::<dyn FnMut(PointerEvent)>::new(move |evt: PointerEvent| {
                    let finished = {
                        let state = slider_drag_state.borrow();
                        state
                            .as_ref()
                            .is_some_and(|d| d.pointer_id == evt.pointer_id())
                    };

                    if !finished {
                        return;
                    }

                    let drag = slider_drag_state.borrow_mut().take();
                    let Some(drag) = drag else {
                        return;
                    };

                    *suppress_slider_input.borrow_mut() = false;

                    let data_toml = match textarea_value(&document_for_up, "data_toml") {
                        Ok(v) => v,
                        Err(_) => return,
                    };

                    match update_data_toml_selected_degree(
                        &data_toml,
                        drag.dataset.as_str(),
                        drag.endpoint.as_str(),
                        drag.degree,
                    ) {
                        Ok(new_toml) => {
                            let _ = set_textarea_value(
                                &document_for_up,
                                "data_toml",
                                new_toml.as_str(),
                            );
                            dispatch_input_event(&document_for_up, "data_toml");
                        }
                        Err(msg) => {
                            set_error(&document_for_up, Some(msg.as_str()));
                        }
                    }

                    evt.prevent_default();
                });

                slider_input
                    .add_event_listener_with_callback("pointerup", cb.as_ref().unchecked_ref())
                    .ok();
                slider_input
                    .add_event_listener_with_callback("pointercancel", cb.as_ref().unchecked_ref())
                    .ok();
                cb.forget();
            }
        }
    }

    // Click-to-select inspector.
    set_selection_text(&document, "(no selection)");

    if let Some(chart_el) = document.get_element_by_id("chart") {
        let document_for_click = document.clone();
        let selection_state = selection_state.clone();
        let suppress_next_click = suppress_next_click.clone();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |evt: web_sys::Event| {
            // If we just completed a drag, the browser still dispatches a click; suppress it so we
            // don't fight with the inspector selection logic.
            {
                let mut suppress = suppress_next_click.borrow_mut();
                if *suppress {
                    *suppress = false;
                    return;
                }
            }

            let Some(target) = evt.target() else {
                set_selection_text(&document_for_click, "(no selection)");
                update_placement_editor_ui(&document_for_click, None);
                update_structure_panel_ui(&document_for_click, None);
                *selection_state.borrow_mut() = None;
                return;
            };

            match selection_from_target(target) {
                Some(sel) => {
                    let summary = selection_summary(&sel);
                    set_selection_text(&document_for_click, summary.as_str());
                    update_placement_editor_ui(&document_for_click, Some(&sel));
                    update_structure_panel_ui(&document_for_click, Some(&sel));
                    *selection_state.borrow_mut() = Some(sel);
                }
                None => {
                    set_selection_text(&document_for_click, "(no selection)");
                    update_placement_editor_ui(&document_for_click, None);
                    update_structure_panel_ui(&document_for_click, None);
                    *selection_state.borrow_mut() = None;
                }
            }
        });

        chart_el.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Aspect toggles → mutate aspects TOML → re-render.
    if let Some(toggle_el) = document.get_element_by_id("aspect_toggles") {
        let document_for_toggle = document.clone();
        let window_for_toggle = window.clone();

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |evt: web_sys::Event| {
            let Some(target) = evt.target() else {
                return;
            };

            let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
                return;
            };

            let Some(kind_name) = input.get_attribute("data-rb-aspect-kind") else {
                return;
            };

            let enabled = input.checked();

            let aspects_toml = match textarea_value(&document_for_toggle, "aspects_toml") {
                Ok(v) => v,
                Err(_) => return,
            };

            match update_aspects_toml_aspect_enabled(&aspects_toml, kind_name.as_str(), enabled) {
                Ok(new_toml) => {
                    let _ =
                        set_textarea_value(&document_for_toggle, "aspects_toml", new_toml.as_str());
                    dispatch_input_event(&document_for_toggle, "aspects_toml");
                }
                Err(msg) => {
                    web_sys::console::error_1(&msg.into());
                }
            }

            // Minor: keep focus on the toggle input for keyboard users.
            let _ = window_for_toggle;
        });

        toggle_el.add_event_listener_with_callback("change", cb.as_ref().unchecked_ref())?;
        cb.forget();
    }

    // Placement editor → mutate data TOML → re-render.
    //
    // We update the raw data TOML and then dispatch an `input` event on #data_toml so the existing
    // debounced render handler re-runs.
    for (id, handler_kind) in [
        ("placement_deg", "deg"),
        ("placement_deg_slider", "deg_slider"),
        ("placement_sign", "sign"),
        ("placement_sign_deg", "sign_deg"),
        ("step_size", "step_range"),
        ("step_size_num", "step_num"),
    ] {
        if let Some(el) = document.get_element_by_id(id) {
            let document_for_change = document.clone();
            let selection_state = selection_state.clone();

            let cb = Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::Event| {
                // Step size changes should always update the `step` attribute on our number inputs,
                // even if nothing is selected.
                if handler_kind == "step_range" || handler_kind == "step_num" {
                    // Important: if the slider changes, prefer reading the slider value (otherwise
                    // the numeric input "wins" and the slider appears to do nothing).
                    let raw = if handler_kind == "step_range" {
                        get_input_f64(&document_for_change, "step_size")
                    } else {
                        get_input_f64(&document_for_change, "step_size_num")
                    }
                    .unwrap_or(1.0);

                    let step_deg = clamp_step_size_deg(raw);

                    // Keep both controls visually in sync.
                    let step_s = format_float_compact(step_deg);
                    set_input_value_string(&document_for_change, "step_size", step_s.as_str());
                    set_input_value_string(&document_for_change, "step_size_num", step_s.as_str());

                    apply_step_size_to_placement_inputs(&document_for_change, step_deg);
                    return;
                }

                let Some(sel) = selection_state.borrow().clone() else {
                    return;
                };

                if sel.kind != SelectionKind::Placement {
                    return;
                }

                let Some(dataset) = parse_dataset_attr(&sel.attrs) else {
                    return;
                };

                let Some(endpoint) = parse_endpoint_attr(&sel.attrs) else {
                    return;
                };

                let mut new_deg: Option<f64> = None;

                match handler_kind {
                    "deg" => {
                        let Some(deg) = get_input_f64(&document_for_change, "placement_deg") else {
                            return;
                        };

                        // Normalize into [0, 360).
                        let d = normalize_deg_0_360(deg);

                        let (sign, in_sign) = degrees_to_sign_parts(d);
                        set_select_value(
                            &document_for_change,
                            "placement_sign",
                            format!("{sign:?}").as_str(),
                        );
                        set_input_value(&document_for_change, "placement_sign_deg", in_sign);
                        set_input_value_string(
                            &document_for_change,
                            "placement_deg_slider",
                            format_float_compact(d).as_str(),
                        );

                        new_deg = Some(d);
                    }
                    "deg_slider" => {
                        let Some(deg) = get_input_f64(&document_for_change, "placement_deg_slider")
                        else {
                            return;
                        };

                        let d = normalize_deg_0_360(deg);
                        let (sign, in_sign) = degrees_to_sign_parts(d);

                        set_input_value(&document_for_change, "placement_deg", d);
                        set_select_value(
                            &document_for_change,
                            "placement_sign",
                            format!("{sign:?}").as_str(),
                        );
                        set_input_value(&document_for_change, "placement_sign_deg", in_sign);

                        new_deg = Some(d);
                    }
                    "sign" | "sign_deg" => {
                        let Some(el) = document_for_change.get_element_by_id("placement_sign")
                        else {
                            return;
                        };
                        let Ok(select) = el.dyn_into::<HtmlSelectElement>() else {
                            return;
                        };

                        let Some(sign) = sign_from_label(select.value().as_str()) else {
                            return;
                        };

                        let Some(in_sign_deg) =
                            get_input_f64(&document_for_change, "placement_sign_deg")
                        else {
                            return;
                        };

                        // Allow in-sign degrees to roll across sign boundaries (no clamping).
                        // Example: Aries 31° -> Taurus 1°.
                        let abs = sign.start_degree_f64() + in_sign_deg;
                        let d = normalize_deg_0_360(abs);
                        let (sign2, in_sign2) = degrees_to_sign_parts(d);

                        // Keep the UI fields consistent with the normalized degree.
                        set_select_value(
                            &document_for_change,
                            "placement_sign",
                            format!("{sign2:?}").as_str(),
                        );
                        set_input_value(&document_for_change, "placement_sign_deg", in_sign2);
                        set_input_value(&document_for_change, "placement_deg", d);
                        set_input_value_string(
                            &document_for_change,
                            "placement_deg_slider",
                            format_float_compact(d).as_str(),
                        );
                        new_deg = Some(d);
                    }
                    _ => {}
                }

                let Some(new_deg) = new_deg else {
                    return;
                };

                // Update the stored selection metadata so the inspector stays in sync.
                {
                    let mut state = selection_state.borrow_mut();
                    if let Some(sel) = state.as_mut() {
                        sel.attrs
                            .insert("data-rb-degree".to_owned(), format!("{:.2}", new_deg));
                        let summary = selection_summary(sel);
                        set_selection_text(&document_for_change, summary.as_str());
                    }
                }

                // Update TOML.
                let data_toml = match textarea_value(&document_for_change, "data_toml") {
                    Ok(v) => v,
                    Err(_) => return,
                };

                match update_data_toml_selected_degree(
                    &data_toml,
                    dataset.as_str(),
                    endpoint.as_str(),
                    new_deg,
                ) {
                    Ok(new_toml) => {
                        let _ = set_textarea_value(
                            &document_for_change,
                            "data_toml",
                            new_toml.as_str(),
                        );
                        dispatch_input_event(&document_for_change, "data_toml");
                    }
                    Err(msg) => {
                        set_error(&document_for_change, Some(msg.as_str()));
                    }
                }
            });

            el.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref())?;
            cb.forget();
        }
    }

    // Debounced re-render on input.
    let state = std::rc::Rc::new(std::cell::RefCell::new(None::<i32>));
    let schedule_render = {
        let window = window.clone();
        let state = state.clone();

        move || {
            if let Some(handle) = state.borrow_mut().take() {
                window.clear_timeout_with_handle(handle);
            }

            let cb = {
                let do_render = do_render.clone();
                Closure::<dyn FnMut()>::new(move || {
                    do_render();
                })
            };

            match window.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                250,
            ) {
                Ok(handle) => {
                    *state.borrow_mut() = Some(handle);
                    cb.forget();
                }
                Err(e) => {
                    web_sys::console::error_1(&e);
                }
            }
        }
    };

    for id in ["theme_toml", "layout_toml", "data_toml", "aspects_toml"] {
        if let Some(el) = document.get_element_by_id(id) {
            let cb = {
                let schedule_render = schedule_render.clone();
                Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::Event| {
                    schedule_render();
                })
            };

            el.add_event_listener_with_callback("input", cb.as_ref().unchecked_ref())?;
            cb.forget();
        }
    }

    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    // Trunk injects the module loader in <head>, so `#[wasm_bindgen(start)]` can run before the
    // page body is parsed. Wait for DOMContentLoaded so editor DOM nodes exist before we seed
    // textarea contents (otherwise the editors can appear blank).
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing document"))?;

    if document.ready_state() == "loading" {
        let cb = Closure::<dyn FnMut(_)>::new(move |_evt: web_sys::Event| {
            // If this errors, it will at least show up as an exception in the console.
            let _ = run();
        });

        document
            .add_event_listener_with_callback("DOMContentLoaded", cb.as_ref().unchecked_ref())?;
        cb.forget();
        Ok(())
    } else {
        run()
    }
}
