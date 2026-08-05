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
        }
    }
}

fn usize_at(v: &Value, key: &str, current: usize) -> usize {
    match v.path(key).and_then(Value::as_f64) {
        Some(n) if n >= 0.0 => n as usize,
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

pub fn load() -> Config {
    let mut cfg = Config::default();

    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return cfg,
    };
    let raw = match std::fs::read_to_string(format!("{home}/.claude/statusline.json")) {
        Ok(r) => r,
        Err(_) => return cfg,
    };
    let v = match json::parse(raw.trim()) {
        Some(v @ Value::Obj(_)) => v,
        _ => return cfg,
    };

    if let Some(name) = v.path("theme").and_then(Value::as_str) {
        cfg.palette = theme::by_name(&name.trim().to_lowercase());
    }

    // Per-slot overrides applied on top of the chosen theme.
    if let Some(Value::Obj(map)) = v.path("colors") {
        for (slot, raw) in map {
            let Some(rgb) = parse_colour(raw) else { continue };
            let p = &mut cfg.palette;
            match slot.as_str() {
                "separator" => p.separator = rgb,
                "dim" => p.dim = rgb,
                "muted" => p.muted = rgb,
                "bright" => p.bright = rgb,
                "brightest" => p.brightest = rgb,
                "accent_a" => p.accent_a = rgb,
                "accent_b" => p.accent_b = rgb,
                "accent_c" => p.accent_c = rgb,
                "model" => p.model = rgb,
                "good" => p.good = rgb,
                "warn" => p.warn = rgb,
                "bad" => p.bad = rgb,
                "notice" => p.notice = rgb,
                _ => {}
            }
        }
    }

    cfg.line1 = segments_at(&v, "line1", cfg.line1);
    cfg.line2 = segments_at(&v, "line2", cfg.line2);
    cfg.bar_width = usize_at(&v, "bar_width", cfg.bar_width).max(1);
    cfg.limit_bar_width = usize_at(&v, "limit_bar_width", cfg.limit_bar_width).max(1);
    cfg.session_name_width = usize_at(&v, "session_name_width", cfg.session_name_width).max(2);
    cfg.bottom_spacing = usize_at(&v, "bottom_spacing", cfg.bottom_spacing);

    if let Some(gap) = v.path("gap").and_then(Value::as_str) {
        let gap = gap.trim().to_lowercase();
        if ["rule", "blank", "none"].contains(&gap.as_str()) {
            cfg.gap = gap;
        }
    }
    if let Some(ch) = v.path("gap_char").and_then(Value::as_str) {
        if !ch.is_empty() {
            cfg.gap_char = ch.to_string();
        }
    }
    if let Some(scale) = v.path("gap_scale").and_then(Value::as_f64) {
        if scale > 0.0 && scale <= 1.0 {
            cfg.gap_scale = scale;
        }
    }
    match v.path("truecolor") {
        Some(Value::Bool(b)) => cfg.truecolor = Some(*b),
        _ => {}
    }

    cfg
}
