//! Claude Code status line, Nord palette.
//!
//! Line 1: session name, model, project, branch/worktree, open PR, effort mode
//! Line 2: context usage, 5h + 7d rate limits, session cost
//!
//! Ported from statusline.js, which was ported from statusline.py. The status
//! line re-runs on every event in every open session, so with several agents
//! working at once the per-render cost multiplies. Interpreter startup was the
//! whole cost: python ~112 ms, bun ~22 ms, this ~2 ms of CPU per render.
//!
//! Output must stay byte-identical to statusline.js. See the parity harness.

mod json;

use json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const RESET: &str = "\x1b[0m";

// Nord palette, https://www.nordtheme.com/docs/colors-and-palettes
const POLAR2: Rgb = (0x43, 0x4C, 0x5E);
const POLAR3: Rgb = (0x4C, 0x56, 0x6A);
const SNOW4: Rgb = (0xD8, 0xDE, 0xE9);
const SNOW6: Rgb = (0xEC, 0xEF, 0xF4);
// Not a Nord token. Sits between polar3 and snow4, which have a large gap with
// nothing between them, for secondary text that still needs reading.
const MUTED: Rgb = (0x7D, 0x86, 0x96);
const FROST7: Rgb = (0x8F, 0xBC, 0xBB);
const FROST8: Rgb = (0x88, 0xC0, 0xD0);
const FROST9: Rgb = (0x81, 0xA1, 0xC1);
const RED: Rgb = (0xBF, 0x61, 0x6A);
const ORANGE: Rgb = (0xD0, 0x87, 0x70);
const YELLOW: Rgb = (0xEB, 0xCB, 0x8B);
const GREEN: Rgb = (0xA3, 0xBE, 0x8C);
const PURPLE: Rgb = (0xB4, 0x8E, 0xAD);

type Rgb = (u8, u8, u8);

// Meter gradient: position along the bar maps to colour.
const GRADIENT: [(f64, Rgb); 4] = [
    (0.00, FROST8),
    (0.40, GREEN),
    (0.70, YELLOW),
    (1.00, RED),
];

const EIGHTHS: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
const TRACK: &str = "░";

const BAR_WIDTH: usize = 16;
const LIMIT_BAR_WIDTH: usize = 6;

// Session name, truncated. AI-generated titles run long, and this leads the row.
const SESSION_NAME_WIDTH: usize = 24;

// U+2800 renders blank but is not whitespace, so it survives trimming.
const BLANK_ROW: &str = "⠀";

// Gap between the identity and usage rows. A terminal row is the smallest unit,
// so there is no half-height gap; a faint rule reads smaller than empty space.
//   "rule"  — dim dotted line, the tightest-looking gap
//   "blank" — fully empty row, the largest gap
//   "none"  — rows touch
const GAP_STYLE: &str = "rule";
const GAP_RULE_CHAR: &str = "┈";
const GAP_RULE_SCALE: f64 = 1.0;

// Blank rows printed below the bar, to keep it off the mode indicator. 0 disables.
const BOTTOM_SPACING: usize = 1;

fn pr_state(state: Option<&str>) -> (Rgb, &'static str) {
    match state {
        Some("approved") => (GREEN, "✓"),
        Some("changes_requested") => (RED, "✗"),
        Some("pending") => (YELLOW, "●"),
        Some("draft") => (POLAR3, "○"),
        _ => (POLAR3, ""),
    }
}

/// Nearest xterm-256 index for an RGB triple: the 6x6x6 cube, or the grey ramp
/// when the channels are close enough to neutral for grey to be a better match.
fn to_256(c: Rgb) -> u8 {
    let (r, g, b) = (c.0 as i32, c.1 as i32, c.2 as i32);

    let cube_axis = |v: i32| -> i32 {
        // Cube levels are 0, 95, 135, 175, 215, 255.
        const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
        let mut best = 0;
        for (i, l) in LEVELS.iter().enumerate() {
            if (v - l).abs() < (v - LEVELS[best]).abs() {
                best = i;
            }
        }
        best as i32
    };
    let (ri, gi, bi) = (cube_axis(r), cube_axis(g), cube_axis(b));
    const LEVELS: [i32; 6] = [0, 95, 135, 175, 215, 255];
    let cube_err = (r - LEVELS[ri as usize]).pow(2)
        + (g - LEVELS[gi as usize]).pow(2)
        + (b - LEVELS[bi as usize]).pow(2);

    // Grey ramp is 232..=255 at 8 + 10n.
    let avg = (r + g + b) / 3;
    let grey_i = (((avg - 8) as f64 / 10.0).round() as i32).clamp(0, 23);
    let grey_v = 8 + grey_i * 10;
    let grey_err = (r - grey_v).pow(2) + (g - grey_v).pow(2) + (b - grey_v).pow(2);

    if grey_err < cube_err {
        (232 + grey_i) as u8
    } else {
        (16 + 36 * ri + 6 * gi + bi) as u8
    }
}

/// True when the terminal advertises 24-bit colour. Everything else gets the
/// 256-colour approximation, which every terminal worth using supports.
/// Cached because fg() is called once per meter cell.
fn truecolor() -> bool {
    static TRUECOLOR: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TRUECOLOR.get_or_init(|| {
        matches!(
            std::env::var("COLORTERM").as_deref(),
            Ok("truecolor") | Ok("24bit")
        )
    })
}

fn fg(c: Rgb) -> String {
    if truecolor() {
        format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
    } else {
        format!("\x1b[38;5;{}m", to_256(c))
    }
}

fn paint(text: &str, c: Rgb) -> String {
    format!("{}{}{}", fg(c), text, RESET)
}

/// Visible column count, ignoring ANSI colour escapes.
fn display_width(text: &str) -> usize {
    let mut n = 0usize;
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip "[...m"
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

/// Python's round() and JS's banker's-rounding shim: half goes to even.
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

fn gradient_at(t: f64) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    for pair in GRADIENT.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if t0 <= t && t <= t1 {
            let span = t1 - t0;
            let f = if span == 0.0 { 0.0 } else { (t - t0) / span };
            return (
                round_half_even(c0.0 as f64 + (c1.0 as f64 - c0.0 as f64) * f) as u8,
                round_half_even(c0.1 as f64 + (c1.1 as f64 - c0.1 as f64) * f) as u8,
                round_half_even(c0.2 as f64 + (c1.2 as f64 - c0.2 as f64) * f) as u8,
            );
        }
    }
    GRADIENT[GRADIENT.len() - 1].1
}

/// Gradient bar accurate to an eighth of a cell.
fn meter(pct: i64, width: usize) -> String {
    let raw = round_half_even(pct as f64 / 100.0 * width as f64 * 8.0);
    let eighths = raw.clamp(0.0, (width * 8) as f64) as usize;
    let full = eighths / 8;
    let remainder = eighths % 8;

    let mut out = String::new();
    for i in 0..full {
        out.push_str(&fg(gradient_at((i as f64 + 0.5) / width as f64)));
        out.push('█');
    }
    let mut filled = full;

    if remainder > 0 && full < width {
        out.push_str(&fg(gradient_at((full as f64 + 0.5) / width as f64)));
        out.push_str(EIGHTHS[remainder]);
        filled += 1;
    }

    if filled < width {
        out.push_str(&fg(POLAR3));
        for _ in 0..(width - filled) {
            out.push_str(TRACK);
        }
    }

    out.push_str(RESET);
    out
}

/// Rust's float formatter already matches Python's f"{x:.2f}" on the awkward
/// cases (0.005 -> 0.01, 2.675 -> 2.67, banker's 1.25 -> 1.2), because both
/// round the true decimal expansion of the double. Scaling by a power of ten
/// first does not: 0.005 * 100 lands on exactly 0.5 and loses the tie-break.
fn format_fixed(x: f64, places: usize) -> String {
    format!("{:.*}", places, x)
}

fn fmt_tokens(n: f64) -> String {
    if n >= 1_000_000.0 {
        let s = format!("{}M", format_fixed(n / 1_000_000.0, 1));
        return s.replace(".0M", "M");
    }
    if n >= 1_000.0 {
        return format!("{}k", round_half_even(n / 1_000.0) as i64);
    }
    // Python prints the int it parsed; these are always whole here.
    format!("{}", n as i64)
}

/// Countdown whose precision matches its horizon.
///
/// Minutes matter on the 5h window, where you might wait it out. On the 7d
/// window they are noise, and hours alone force a mental divide by 24.
/// Spaces between units because "121h25m" runs together at a glance.
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

/// Current branch, read straight from .git/HEAD.
///
/// Forking `git` dominated the original script's runtime, and the status line
/// re-renders constantly. Two file reads cost a fraction of that, and unlike a
/// cache they can never go stale.
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
        let target = pointer[pointer.find(':')? + 1..].trim();
        let target = Path::new(target);
        dotgit = if target.is_absolute() {
            target.to_path_buf()
        } else {
            path.join(target)
        };
    }

    let head = std::fs::read_to_string(dotgit.join("HEAD")).ok()?;
    let head = head.trim();

    const PREFIX: &str = "ref: refs/heads/";
    let name = if let Some(rest) = head.strip_prefix(PREFIX) {
        rest.to_string()
    } else {
        // Detached HEAD, mid-rebase or on a tag: a short SHA beats nothing.
        head.chars().take(7).collect()
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
fn location(data: &Value, cwd: &str) -> String {
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
        // Directory is the worktree; mark it once instead of naming it twice.
        segments.push(paint(&format!("⑂ {}", place), FROST8));
    } else {
        if !place.is_empty() {
            segments.push(paint(place, FROST8));
        }
        if let Some(t) = tree {
            segments.push(paint(&format!("⑂ {}", t), FROST7));
        }
    }

    if let Some(b) = branch.as_deref() {
        if b != place && Some(b) != tree {
            segments.push(paint(&format!("⎇ {}", b), FROST9));
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

/// Prefer the herdr workspace label, which is what the tab bar shows.
fn session_label(data: &Value) -> Option<String> {
    let name = herdr_workspace_name().or_else(|| data.str_path("session_name"))?;
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > SESSION_NAME_WIDTH {
        let head: String = chars[..SESSION_NAME_WIDTH - 1].iter().collect();
        return Some(format!("{}…", head.trim_end()));
    }
    Some(name)
}

fn join_parts(parts: &[String]) -> String {
    let sep = format!(" {}│{} ", fg(POLAR2), RESET);
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(&sep)
}

fn identity_line(data: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Leads the row: it is the only segment that differs between panes on the
    // same project, so it is what the eye needs first when scanning tabs.
    if let Some(name) = session_label(data) {
        parts.push(paint(&name, SNOW6));
    }

    if let Some(model) = data.path("model.display_name").and_then(Value::as_str) {
        parts.push(paint(model, PURPLE));
    }

    let cwd = data
        .path("workspace.current_dir")
        .and_then(Value::as_str)
        .or_else(|| data.path("cwd").and_then(Value::as_str))
        .unwrap_or("");

    parts.push(location(data, cwd));

    if let Some(num) = data.path("pr.number") {
        if num.is_truthy() {
            let (colour, icon) = pr_state(data.path("pr.review_state").and_then(Value::as_str));
            let raw = num.as_raw().unwrap_or("");
            let text = if icon.is_empty() {
                format!("PR #{}", raw)
            } else {
                format!("PR #{} {}", raw, icon)
            };
            parts.push(paint(&text, colour));
        }
    }

    let mut modes: Vec<String> = Vec::new();
    if let Some(effort) = data.path("effort.level").and_then(Value::as_str) {
        modes.push(paint(effort, MUTED));
    }
    if data.path("fast_mode").map(Value::is_truthy).unwrap_or(false) {
        modes.push(paint("fast", ORANGE));
    }
    if !modes.is_empty() {
        parts.push(modes.join(" "));
    }

    join_parts(&parts)
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn usage_line(data: &Value) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(pct_val) = data.path("context_window.used_percentage").and_then(Value::as_f64) {
        let pct = pct_val.trunc() as i64;
        let tip = fg(gradient_at(pct as f64 / 100.0));
        let mut segment = format!("{} {}{}%{}", meter(pct, BAR_WIDTH), tip, pct, RESET);
        // used_percentage counts input tokens only (input + cache create + cache
        // read), so the token figure uses the same basis rather than adding output.
        let used = data
            .path("context_window.total_input_tokens")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let size = data
            .path("context_window.context_window_size")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if used != 0.0 && size != 0.0 {
            segment.push(' ');
            segment.push_str(&paint(
                &format!("{}/{}", fmt_tokens(used), fmt_tokens(size)),
                MUTED,
            ));
        }
        parts.push(segment);
    }

    for (key, label) in [("five_hour", "5h"), ("seven_day", "7d")] {
        let pct_val = match data
            .path(&format!("rate_limits.{}.used_percentage", key))
            .and_then(Value::as_f64)
        {
            Some(v) => v,
            None => continue,
        };
        let pct = pct_val.trunc() as i64;
        let tip = fg(gradient_at(pct as f64 / 100.0));
        let mut segment = format!(
            "{} {} {}{}%{}",
            paint(label, MUTED),
            meter(pct, LIMIT_BAR_WIDTH),
            tip,
            pct,
            RESET
        );
        if let Some(resets) = data
            .path(&format!("rate_limits.{}.resets_at", key))
            .and_then(Value::as_f64)
        {
            if resets != 0.0 {
                segment.push(' ');
                segment.push_str(&paint(&fmt_duration(resets - now_secs()), MUTED));
            }
        }
        parts.push(segment);
    }

    if let Some(cost) = data.path("cost.total_cost_usd").and_then(Value::as_f64) {
        if cost != 0.0 {
            parts.push(paint(&format!("${}", format_fixed(cost, 2)), SNOW4));
        }
    }

    join_parts(&parts)
}

fn main() {
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        return;
    }

    let data = match json::parse(raw.trim()) {
        Some(v @ Value::Obj(_)) => v,
        // Non-object JSON and malformed input both render nothing, as before.
        _ => return,
    };

    let lines: Vec<String> = [identity_line(&data), usage_line(&data)]
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect();
    if lines.is_empty() {
        return;
    }

    let gap: Vec<String> = match GAP_STYLE {
        "none" => Vec::new(),
        "rule" => {
            let span = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
            let width = (round_half_even(span as f64 * GAP_RULE_SCALE) as usize).max(1);
            vec![format!(
                "{}{}{}",
                fg(POLAR2),
                GAP_RULE_CHAR.repeat(width),
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
    for _ in 0..BOTTOM_SPACING {
        rows.push(BLANK_ROW.to_string());
    }

    let out = rows.join("\n") + "\n";
    let _ = std::io::stdout().write_all(out.as_bytes());
}
