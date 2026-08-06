//! User config, read from ~/.claude/statusline.json.
//!
//! Absent or malformed means defaults, never an error: a status line that
//! disappears because of a stray comma is worse than one that ignores you.

use crate::json::{self, Value};
use crate::theme::{self, Palette, Rgb};

pub struct Config {
    pub palette: Palette,
    pub line1: Vec<String>,
    pub line2: Vec<String>,
    pub bar_width: usize,
    pub limit_bar_width: usize,
    pub session_name_width: usize,
    /// "rule", "blank" or "none".
    pub gap: String,
    pub gap_char: String,
    pub gap_scale: f64,
    pub bottom_spacing: usize,
    /// None means auto-detect from COLORTERM.
    pub truecolor: Option<bool>,
    /// Drop lower-priority segments rather than let a row wrap. 0 disables.
    /// Otherwise the width comes from $COLUMNS, which Claude Code sets.
    pub responsive: bool,
    /// Order segments are shed in when the row will not fit. First to go, first
    /// in the list. Anything absent here is kept.
    pub drop_order: Vec<String>,
    /// Internal, never read from JSON. Preview turns it off so a sample bar
    /// shows the sample's session name rather than the caller's real workspace.
    pub use_herdr: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            palette: theme::by_name("nord"),
            line1: ["session", "model", "location", "pr", "modes"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            line2: ["context", "limits", "cost"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            bar_width: 16,
            limit_bar_width: 6,
            session_name_width: 24,
            gap: "rule".to_string(),
            gap_char: "┈".to_string(),
            gap_scale: 1.0,
            bottom_spacing: 1,
            truecolor: None,
            responsive: true,
            // Cost and model go first: cost is glanceable elsewhere and the
            // model is identical in every session. Context and location are
            // the last things worth losing.
            drop_order: ["cost", "modes", "model", "pr", "limits", "session"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            use_herdr: true,
        }
    }
}

/// Reads a whole number within [floor, ceiling], ignoring anything outside.
///
/// The ceiling matters: without one, `"bar_width": 100000000` is accepted and
/// the renderer then allocates until the process stalls. Nothing here should be
/// larger than a terminal is wide, so the caps are generous but finite.
fn usize_at(v: &Value, key: &str, current: usize, floor: usize, ceiling: usize) -> usize {
    match v.path(key).and_then(Value::as_f64) {
        Some(n) if n.is_finite() && n >= 0.0 && n <= ceiling as f64 => {
            (n as usize).clamp(floor, ceiling)
        }
        _ => current,
    }
}

/// "#88C0D0", "#8cd" or [136, 192, 208].
fn parse_colour(v: &Value) -> Option<Rgb> {
    if let Some(items) = v.as_arr() {
        if items.len() == 3 {
            let c: Vec<u8> = items
                .iter()
                .filter_map(Value::as_f64)
                .map(|n| n.clamp(0.0, 255.0) as u8)
                .collect();
            if c.len() == 3 {
                return Some((c[0], c[1], c[2]));
            }
        }
        return None;
    }
    let s = v.as_str()?.trim().trim_start_matches('#');
    // len() counts bytes and the slicing below assumes they are characters.
    // Without this, six bytes of multi-byte text ("€€") splits mid-character
    // and aborts the process, which is exactly the "malformed means defaults"
    // promise this file opens with. Note "ééé" happens to land on a valid
    // boundary, so this survives casual testing.
    if !s.is_ascii() {
        return None;
    }
    let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
    match s.len() {
        3 => {
            let mut it = s.chars();
            Some((expand(it.next()?)?, expand(it.next()?)?, expand(it.next()?)?))
        }
        6 => Some((
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        )),
        _ => None,
    }
}

fn segments_at(v: &Value, key: &str, current: Vec<String>) -> Vec<String> {
    match v.path(key).and_then(Value::as_arr) {
        Some(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.trim().to_lowercase())
            .collect(),
        None => current,
    }
}

/// Presets are starting points, applied before the individual keys so anything
/// you set explicitly still wins. "compact" collapses the whole thing to one
/// row for people who do not want four lines of chrome above the prompt.
pub fn apply_preset(cfg: &mut Config, name: &str) {
    // "full" is the default shape, named so it can be stated explicitly, and
    // an unrecognised name is ignored the same way.
    if name == "compact" {
        // Everything on one row. Model is dropped rather than shortened: it is
        // identical in every session, so it is the least informative thing
        // competing for the space.
        cfg.line1 = ["session", "location", "context", "limits", "cost"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        cfg.line2 = Vec::new();
        cfg.bar_width = 10;
        cfg.limit_bar_width = 4;
        cfg.gap = "none".to_string();
        cfg.bottom_spacing = 0;
    }
}

pub fn load() -> Config {
    let mut cfg = Config::default();

    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return cfg,
    };
    let path = format!("{home}/.claude/statusline.json");
    let Some(raw) = crate::read_capped(std::path::Path::new(&path), crate::LARGE_FILE) else {
        return cfg;
    };
    let v = match json::parse(raw.trim()) {
        Some(v @ Value::Obj(_)) => v,
        _ => return cfg,
    };

    // Before everything else, so explicit keys override the preset.
    if let Some(preset) = v.path("preset").and_then(Value::as_str) {
        apply_preset(&mut cfg, &preset.trim().to_lowercase());
    }

    if let Some(name) = v.path("theme").and_then(Value::as_str) {
        cfg.palette = theme::by_name(&name.trim().to_lowercase());
    }

    // Per-slot overrides applied on top of the chosen theme.
    if let Some(Value::Obj(map)) = v.path("colors") {
        for (slot, raw) in map {
            let Some(rgb) = parse_colour(raw) else { continue };
            // Unknown slot names are ignored, same as unknown segment names.
            if let Some((_, set)) = theme::SLOTS.iter().find(|(n, _)| *n == slot.as_str()) {
                set(&mut cfg.palette, rgb);
            }
        }
    }

    cfg.line1 = segments_at(&v, "line1", cfg.line1);
    cfg.line2 = segments_at(&v, "line2", cfg.line2);
    cfg.bar_width = usize_at(&v, "bar_width", cfg.bar_width, 1, 500);
    cfg.limit_bar_width = usize_at(&v, "limit_bar_width", cfg.limit_bar_width, 1, 500);
    cfg.session_name_width = usize_at(&v, "session_name_width", cfg.session_name_width, 2, 500);
    cfg.bottom_spacing = usize_at(&v, "bottom_spacing", cfg.bottom_spacing, 0, 20);

    if let Some(gap) = v.path("gap").and_then(Value::as_str) {
        let gap = gap.trim().to_lowercase();
        if ["rule", "blank", "none"].contains(&gap.as_str()) {
            cfg.gap = gap;
        }
    }
    if let Some(ch) = v.path("gap_char").and_then(Value::as_str) {
        // One character, and not a control one. The rule is emitted without
        // going through paint(), so this is the only place it can be filtered,
        // and it repeats to the width of the row, so a long string here
        // multiplies into megabytes of output.
        if let Some(first) = ch.chars().next() {
            let n = first as u32;
            if !(n < 0x20 || n == 0x7F || (0x80..=0x9F).contains(&n)) {
                cfg.gap_char = first.to_string();
            }
        }
    }
    if let Some(scale) = v.path("gap_scale").and_then(Value::as_f64) {
        if scale > 0.0 && scale <= 1.0 {
            cfg.gap_scale = scale;
        }
    }
    if let Some(Value::Bool(b)) = v.path("truecolor") {
        cfg.truecolor = Some(*b);
    }
    if let Some(Value::Bool(b)) = v.path("responsive") {
        cfg.responsive = *b;
    }
    if v.path("drop_order").and_then(Value::as_arr).is_some() {
        cfg.drop_order = segments_at(&v, "drop_order", cfg.drop_order.clone());
    }

    cfg
}
