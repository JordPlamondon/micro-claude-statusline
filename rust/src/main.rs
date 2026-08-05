//! Claude Code status line.
//!
//! Reads the statusLine JSON payload on stdin, prints a two-row bar. Segments,
//! widths and palette come from ~/.claude/statusline.json when it exists.
//!
//! The status line re-runs on every event in every open session, so with several
//! agents working at once the per-render cost multiplies. Interpreter startup was
//! the whole cost of earlier versions: python ~114 ms, bun ~24 ms, this ~3 ms.
//!
//! Output must stay byte-identical to the js and python implementations. See
//! scripts/parity.py.

mod config;
mod json;
mod theme;

use config::Config;
use json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use theme::Rgb;

const RESET: &str = "\x1b[0m";
const EIGHTHS: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
const TRACK: &str = "░";
// U+2800 renders blank but is not whitespace, so it survives trimming.
const BLANK_ROW: &str = "⠀";

fn pr_state(state: Option<&str>, p: &theme::Palette) -> (Rgb, &'static str) {
    match state {
        Some("approved") => (p.good, "✓"),
        Some("changes_requested") => (p.bad, "✗"),
        Some("pending") => (p.warn, "●"),
        Some("draft") => (p.dim, "○"),
        _ => (p.dim, ""),
    }
}

/// Nearest xterm-256 index for an RGB triple: the 6x6x6 cube, or the grey ramp
/// when the channels are close enough to neutral for grey to be a better match.
fn to_256(c: Rgb) -> u8 {
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let (r, g, b) = (c.0 as i32, c.1 as i32, c.2 as i32);

    let axis = |v: i32| -> usize {
        let mut best = 0;
        for (i, l) in LEVELS.iter().enumerate() {
            if (v - l).abs() < (v - LEVELS[best]).abs() {
                best = i;
            }
        }
        best
    };
    let (ri, gi, bi) = (axis(r), axis(g), axis(b));
    let cube_err =
        (r - LEVELS[ri]).pow(2) + (g - LEVELS[gi]).pow(2) + (b - LEVELS[bi]).pow(2);

    // Grey ramp is 232..=255 at 8 + 10n.
    let avg = (r + g + b) / 3;
    let grey_i = (((avg - 8) as f64 / 10.0).round() as i32).clamp(0, 23);
    let grey_v = 8 + grey_i * 10;
    let grey_err = (r - grey_v).pow(2) + (g - grey_v).pow(2) + (b - grey_v).pow(2);

    if grey_err < cube_err {
        (232 + grey_i) as u8
    } else {
        (16 + 36 * ri as i32 + 6 * gi as i32 + bi as i32) as u8
    }
}

/// Rendering state threaded through the segment builders.
struct Ctx<'a> {
    cfg: &'a Config,
    truecolor: bool,
}

impl<'a> Ctx<'a> {
    fn fg(&self, c: Rgb) -> String {
        if self.truecolor {
            format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
        } else {
            format!("\x1b[38;5;{}m", to_256(c))
        }
    }

    fn paint(&self, text: &str, c: Rgb) -> String {
        format!("{}{}{}", self.fg(c), text, RESET)
    }

    fn gradient_at(&self, t: f64) -> Rgb {
        let stops = self.cfg.palette.gradient();
        let t = t.clamp(0.0, 1.0);
        for pair in stops.windows(2) {
            let (t0, c0) = pair[0];
            let (t1, c1) = pair[1];
            if t0 <= t && t <= t1 {
                let span = t1 - t0;
                let f = if span == 0.0 { 0.0 } else { (t - t0) / span };
                let mix = |a: u8, b: u8| {
                    round_half_even(a as f64 + (b as f64 - a as f64) * f) as u8
                };
                return (mix(c0.0, c1.0), mix(c0.1, c1.1), mix(c0.2, c1.2));
            }
        }
        stops[stops.len() - 1].1
    }

    /// Gradient bar accurate to an eighth of a cell.
    fn meter(&self, pct: i64, width: usize) -> String {
        let raw = round_half_even(pct as f64 / 100.0 * width as f64 * 8.0);
        let eighths = raw.clamp(0.0, (width * 8) as f64) as usize;
        let full = eighths / 8;
        let remainder = eighths % 8;

        let mut out = String::new();
        for i in 0..full {
            out.push_str(&self.fg(self.gradient_at((i as f64 + 0.5) / width as f64)));
            out.push('█');
        }
        let mut filled = full;

        if remainder > 0 && full < width {
            out.push_str(&self.fg(self.gradient_at((full as f64 + 0.5) / width as f64)));
            out.push_str(EIGHTHS[remainder]);
            filled += 1;
        }

        if filled < width {
            out.push_str(&self.fg(self.cfg.palette.dim));
            for _ in 0..(width - filled) {
                out.push_str(TRACK);
            }
        }

        out.push_str(RESET);
        out
    }
}

/// Visible column count, ignoring ANSI colour escapes.
fn display_width(text: &str) -> usize {
    let mut n = 0usize;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            n += 1;
        }
    }
    n
}

/// Python's round() and the JS shim: half goes to even.
fn round_half_even(x: f64) -> f64 {
    let floor = x.floor();
    let diff = x - floor;
    if diff > 0.5 {
        floor + 1.0
    } else if diff < 0.5 {
        floor
    } else if (floor % 2.0) == 0.0 {
        floor
    } else {
        floor + 1.0
    }
}

/// Rust's float formatter already matches Python's f"{x:.2f}" on the awkward
/// cases (0.005 -> 0.01, 2.675 -> 2.67, banker's 1.25 -> 1.2), because both
/// round the true decimal expansion. Scaling by a power of ten first does not:
/// 0.005 * 100 lands on exactly 0.5 and loses the tie-break.
fn format_fixed(x: f64, places: usize) -> String {
    format!("{:.*}", places, x)
}

fn fmt_tokens(n: f64) -> String {
    if n >= 1_000_000.0 {
        return format!("{}M", format_fixed(n / 1_000_000.0, 1)).replace(".0M", "M");
    }
    if n >= 1_000.0 {
        return format!("{}k", round_half_even(n / 1_000.0) as i64);
    }
    format!("{}", n as i64)
}

/// Countdown whose precision matches its horizon.
///
/// Minutes matter on the 5h window, where you might wait it out. On the 7d
/// window they are noise, and hours alone force a mental divide by 24.
fn fmt_duration(seconds: f64) -> String {
    if seconds <= 0.0 {
        return "now".to_string();
    }
    let total_min = (seconds as i64) / 60;
    let mut hours = total_min / 60;
    let minutes = total_min % 60;
    let days = hours / 24;
    hours %= 24;
    if days > 0 {
        return format!("{}d {}h", days, hours);
    }
    if hours > 0 {
        return format!("{}h {:02}m", hours, minutes);
    }
    format!("{}m", minutes)
}

/// Current branch, read straight from .git/HEAD rather than forking git.
fn git_branch(cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    let mut path: PathBuf = Path::new(cwd).to_path_buf();
    if !path.is_dir() {
        return None;
    }

    // Walk up to the repo. .git is a directory normally, a file in a worktree.
    let mut dotgit;
    loop {
        dotgit = path.join(".git");
        if dotgit.exists() {
            break;
        }
        match path.parent() {
            Some(parent) if parent != path => path = parent.to_path_buf(),
            _ => return None,
        }
    }

    if dotgit.is_file() {
        // Linked worktree: "gitdir: /abs/path/.git/worktrees/<name>"
        let pointer = std::fs::read_to_string(&dotgit).ok()?;
        let pointer = pointer.trim();
        if !pointer.starts_with("gitdir:") {
            return None;
        }
        let target = Path::new(pointer[pointer.find(':')? + 1..].trim());
        dotgit = if target.is_absolute() {
            target.to_path_buf()
        } else {
            path.join(target)
        };
    }

    let head = std::fs::read_to_string(dotgit.join("HEAD")).ok()?;
    let head = head.trim();

    const PREFIX: &str = "ref: refs/heads/";
    let name = match head.strip_prefix(PREFIX) {
        Some(rest) => rest.to_string(),
        // Detached HEAD, mid-rebase or on a tag: a short SHA beats nothing.
        None => head.chars().take(7).collect(),
    };
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Directory, worktree and branch, with repeated names collapsed.
///
/// Worktrees are commonly created with the directory, worktree and branch all
/// sharing one name, which would otherwise print that name three times.
fn location(ctx: &Ctx, data: &Value, cwd: &str) -> String {
    let p = &ctx.cfg.palette;
    let trimmed = cwd.trim_end_matches('/');
    let place = match trimmed.rfind('/') {
        Some(i) => &trimmed[i + 1..],
        None => trimmed,
    };
    // git_worktree lives under workspace; worktree.name is top level.
    let tree = data
        .path("workspace.git_worktree")
        .and_then(Value::as_str)
        .or_else(|| data.path("worktree.name").and_then(Value::as_str));
    let branch = git_branch(cwd);

    let mut segments: Vec<String> = Vec::new();
    if tree == Some(place) {
        segments.push(ctx.paint(&format!("⑂ {}", place), p.accent_b));
    } else {
        if !place.is_empty() {
            segments.push(ctx.paint(place, p.accent_b));
        }
        if let Some(t) = tree {
            segments.push(ctx.paint(&format!("⑂ {}", t), p.accent_a));
        }
    }

    if let Some(b) = branch.as_deref() {
        if b != place && Some(b) != tree {
            segments.push(ctx.paint(&format!("⎇ {}", b), p.accent_c));
        }
    }

    segments.join(" ")
}

/// Label of the herdr workspace this pane belongs to, if it has one.
///
/// Only custom labels are returned. herdr falls back to the directory basename
/// for unlabelled workspaces, which the location segment already shows.
fn herdr_workspace_name() -> Option<String> {
    let id = std::env::var("HERDR_WORKSPACE_ID").ok()?;
    if id.is_empty() {
        return None;
    }
    let home = std::env::var("HOME").ok()?;
    let raw = std::fs::read_to_string(format!("{}/.config/herdr/session.json", home)).ok()?;
    let state = json::parse(&raw)?;
    for workspace in state.path("workspaces")?.as_arr()? {
        if workspace.get("id").and_then(Value::as_str) == Some(id.as_str()) {
            return workspace.str_path("custom_name");
        }
    }
    None
}

/// Prefer the herdr workspace label, which is what its tab bar shows.
fn session_label(data: &Value, width: usize) -> Option<String> {
    let name = herdr_workspace_name().or_else(|| data.str_path("session_name"))?;
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > width {
        let head: String = chars[..width - 1].iter().collect();
        return Some(format!("{}…", head.trim_end()));
    }
    Some(name)
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Renders one named segment, or None when it has no data to show.
fn segment(name: &str, ctx: &Ctx, data: &Value) -> Option<String> {
    let cfg = ctx.cfg;
    let p = &cfg.palette;

    match name {
        "session" => session_label(data, cfg.session_name_width).map(|n| ctx.paint(&n, p.brightest)),

        "model" => data
            .path("model.display_name")
            .and_then(Value::as_str)
            .map(|m| ctx.paint(m, p.model)),

        "location" => {
            let cwd = data
                .path("workspace.current_dir")
                .and_then(Value::as_str)
                .or_else(|| data.path("cwd").and_then(Value::as_str))
                .unwrap_or("");
            let s = location(ctx, data, cwd);
            (!s.is_empty()).then_some(s)
        }

        "pr" => {
            let num = data.path("pr.number")?;
            if !num.is_truthy() {
                return None;
            }
            let (colour, icon) = pr_state(data.path("pr.review_state").and_then(Value::as_str), p);
            let raw = num.as_raw().unwrap_or("");
            let text = if icon.is_empty() {
                format!("PR #{}", raw)
            } else {
                format!("PR #{} {}", raw, icon)
            };
            Some(ctx.paint(&text, colour))
        }

        "modes" => {
            let mut modes: Vec<String> = Vec::new();
            if let Some(effort) = data.path("effort.level").and_then(Value::as_str) {
                modes.push(ctx.paint(effort, p.muted));
            }
            if data.path("fast_mode").map(Value::is_truthy).unwrap_or(false) {
                modes.push(ctx.paint("fast", p.notice));
            }
            (!modes.is_empty()).then(|| modes.join(" "))
        }

        "context" => {
            let pct_val = data
                .path("context_window.used_percentage")
                .and_then(Value::as_f64)?;
            let pct = pct_val.trunc() as i64;
            let tip = ctx.fg(ctx.gradient_at(pct as f64 / 100.0));
            let mut out = format!("{} {}{}%{}", ctx.meter(pct, cfg.bar_width), tip, pct, RESET);
            // used_percentage counts input tokens only (input + cache create +
            // cache read), so the token figure uses the same basis.
            let used = data
                .path("context_window.total_input_tokens")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let size = data
                .path("context_window.context_window_size")
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            if used != 0.0 && size != 0.0 {
                out.push(' ');
                out.push_str(&ctx.paint(
                    &format!("{}/{}", fmt_tokens(used), fmt_tokens(size)),
                    p.muted,
                ));
            }
            Some(out)
        }

        // Both windows in one segment, matching how they read together.
        "limits" | "limit_5h" | "limit_7d" => {
            let wanted: &[(&str, &str)] = match name {
                "limit_5h" => &[("five_hour", "5h")],
                "limit_7d" => &[("seven_day", "7d")],
                _ => &[("five_hour", "5h"), ("seven_day", "7d")],
            };
            let mut out: Vec<String> = Vec::new();
            for (key, label) in wanted {
                let Some(pct_val) = data
                    .path(&format!("rate_limits.{}.used_percentage", key))
                    .and_then(Value::as_f64)
                else {
                    continue;
                };
                let pct = pct_val.trunc() as i64;
                let tip = ctx.fg(ctx.gradient_at(pct as f64 / 100.0));
                let mut seg = format!(
                    "{} {} {}{}%{}",
                    ctx.paint(label, p.muted),
                    ctx.meter(pct, cfg.limit_bar_width),
                    tip,
                    pct,
                    RESET
                );
                if let Some(resets) = data
                    .path(&format!("rate_limits.{}.resets_at", key))
                    .and_then(Value::as_f64)
                {
                    if resets != 0.0 {
                        seg.push(' ');
                        seg.push_str(&ctx.paint(&fmt_duration(resets - now_secs()), p.muted));
                    }
                }
                out.push(seg);
            }
            (!out.is_empty()).then(|| out.join(&format!(" {}│{} ", ctx.fg(p.separator), RESET)))
        }

        "cost" => {
            let cost = data.path("cost.total_cost_usd").and_then(Value::as_f64)?;
            (cost != 0.0).then(|| ctx.paint(&format!("${}", format_fixed(cost, 2)), p.bright))
        }

        // Unknown names are ignored, so a typo costs a segment not the bar.
        _ => None,
    }
}

fn build_line(names: &[String], ctx: &Ctx, data: &Value) -> String {
    let parts: Vec<String> = names
        .iter()
        .filter_map(|n| segment(n, ctx, data))
        .filter(|s| !s.is_empty())
        .collect();
    let sep = format!(" {}│{} ", ctx.fg(ctx.cfg.palette.separator), RESET);
    parts.join(&sep)
}

fn main() {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return;
    }

    let cfg = config::load();
    let ctx = Ctx {
        truecolor: cfg.truecolor.unwrap_or_else(|| {
            matches!(
                std::env::var("COLORTERM").as_deref(),
                Ok("truecolor") | Ok("24bit")
            )
        }),
        cfg: &cfg,
    };

    // --themes prints the available names, for the README and for anyone poking.
    if std::env::args().any(|a| a == "--themes") {
        println!("{}", theme::NAMES.join("\n"));
        return;
    }

    let data = match json::parse(raw.trim()) {
        Some(v @ Value::Obj(_)) => v,
        // Non-object JSON and malformed input both render nothing.
        _ => return,
    };

    let lines: Vec<String> = [
        build_line(&cfg.line1, &ctx, &data),
        build_line(&cfg.line2, &ctx, &data),
    ]
    .into_iter()
    .filter(|l| !l.is_empty())
    .collect();
    if lines.is_empty() {
        return;
    }

    let gap: Vec<String> = match cfg.gap.as_str() {
        "none" => Vec::new(),
        "rule" => {
            let span = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
            let width = (round_half_even(span as f64 * cfg.gap_scale) as usize).max(1);
            vec![format!(
                "{}{}{}",
                ctx.fg(cfg.palette.separator),
                cfg.gap_char.repeat(width),
                RESET
            )]
        }
        _ => vec![BLANK_ROW.to_string()],
    };

    let mut rows: Vec<String> = Vec::new();
    for line in lines {
        if !rows.is_empty() {
            rows.extend(gap.iter().cloned());
        }
        rows.push(line);
    }
    for _ in 0..cfg.bottom_spacing {
        rows.push(BLANK_ROW.to_string());
    }

    let _ = std::io::stdout().write_all((rows.join("\n") + "\n").as_bytes());
}
