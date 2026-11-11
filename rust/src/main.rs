//! Claude Code status line.
//!
//! Reads the statusLine JSON payload on stdin, prints a two-row bar. Segments,
//! widths and palette come from ~/.claude/statusline.json when it exists.
//!
//! The status line re-runs on every event in every open session, so with several
//! agents working at once the per-render cost multiplies. Interpreter startup was
//! the whole cost of earlier versions: python ~114 ms, bun ~24 ms, this ~2.3 ms.
//!
//! Output is pinned by tests/golden.txt. Run scripts/golden.py after any change
//! that touches what gets printed.

// There is no unsafe here and there is no reason for there ever to be any: the
// program reads a string, formats a string and prints it. forbid rather than
// deny, so it cannot be switched off locally by an allow attribute further in.
#![forbid(unsafe_code)]

mod cli;
mod config;
mod json;
mod theme;

use config::{Config, Gap};
use json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use theme::Rgb;

const RESET: &str = "\x1b[0m";
const EIGHTHS: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
/// SGR layers for colour_code.
const FG: u8 = 38;
const BG: u8 = 48;
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
    let cube_err = (r - LEVELS[ri]).pow(2) + (g - LEVELS[gi]).pow(2) + (b - LEVELS[bi]).pow(2);

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
    /// What divides two things on a row. Built once, because both the row
    /// builder and the rate-limit segment need it and two copies of the same
    /// format string is one copy too many to keep in step.
    sep: String,
}

/// Strips control characters, so text we did not write cannot become escapes.
///
/// Directory names, session names, model names and .git/HEAD all reach stdout,
/// and a directory containing an ESC is enough on its own: no compromise of
/// Claude Code is needed, just working somewhere with a hostile name. Left
/// unfiltered, a terminal would act on OSC sequences for window titles and, on
/// terminals that allow it, clipboard writes.
///
/// Also a correctness fix. display_width skips from ESC to the next `m`, so an
/// unterminated escape swallowed the rest of the row, mis-measuring the width
/// and drawing the gap rule short.
///
/// C0 (0x00-0x1F), DEL (0x7F) and C1 (0x80-0x9F) all go. Newline included:
/// rows are joined by this program, not by their contents.
fn strip_controls(text: &str) -> String {
    text.chars().filter(|c| !is_control(*c)).collect()
}

/// The one test for characters that must not reach the terminal. strip_controls
/// and the gap_char check in config.rs share it so the two cannot drift apart.
///
/// Unicode bidi controls are included: ALM (U+061C), LRM/RLM (U+200E-200F),
/// the embeddings and overrides (U+202A-202E) and the isolates (U+2066-2069).
/// An SGR reset does not close an embedding or override, so a directory named
/// with U+202E reversed every segment after it on the row. The joiners U+200C
/// and U+200D stay: emoji sequences and several scripts need them.
pub fn is_control(c: char) -> bool {
    let n = c as u32;
    n < 0x20
        || n == 0x7F
        || (0x80..=0x9F).contains(&n)
        || n == 0x061C
        || (0x200E..=0x200F).contains(&n)
        || (0x202A..=0x202E).contains(&n)
        || (0x2066..=0x2069).contains(&n)
}

/// Escape for a colour on the FG or BG layer, at the depth the terminal supports.
fn colour_code(layer: u8, c: Rgb, truecolor: bool) -> String {
    if truecolor {
        format!("\x1b[{layer};2;{};{};{}m", c.0, c.1, c.2)
    } else {
        format!("\x1b[{layer};5;{}m", to_256(c))
    }
}

impl<'a> Ctx<'a> {
    fn new(cfg: &'a Config, truecolor: bool) -> Self {
        let sep = format!(
            " {}│{RESET} ",
            colour_code(FG, cfg.palette.separator, truecolor)
        );
        Ctx {
            cfg,
            truecolor,
            sep,
        }
    }

    fn fg(&self, c: Rgb) -> String {
        colour_code(FG, c, self.truecolor)
    }

    fn bg(&self, c: Rgb) -> String {
        colour_code(BG, c, self.truecolor)
    }

    /// The single choke point for anything derived from outside this program.
    /// Meters build their own escapes from block characters, and the PR number
    /// comes from a JSON number token, so neither needs filtering.
    fn paint(&self, text: &str, c: Rgb) -> String {
        format!("{}{}{}", self.fg(c), strip_controls(text), RESET)
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
                let mix =
                    |a: u8, b: u8| round_half_even(a as f64 + (b as f64 - a as f64) * f) as u8;
                return (mix(c0.0, c1.0), mix(c0.1, c1.1), mix(c0.2, c1.2));
            }
        }
        stops[stops.len() - 1].1
    }

    /// Gradient bar accurate to an eighth of a cell.
    ///
    /// The track is background colour, not a glyph, so the cell holding a
    /// partial block shares it. A shaded track glyph left the rest of that cell
    /// bare, which showed as a gap between the fill and the track.
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

        if full < width {
            out.push_str(&self.bg(self.cfg.palette.dim));
            let mut empty = width - full;
            if remainder > 0 {
                out.push_str(&self.fg(self.gradient_at((full as f64 + 0.5) / width as f64)));
                out.push_str(EIGHTHS[remainder]);
                empty -= 1;
            }
            out.push_str(&" ".repeat(empty));
        }

        out.push_str(RESET);
        out
    }
}

/// Terminal columns one character occupies.
///
/// East Asian Wide and Fullwidth characters take two cells. Counting them as
/// one made a row of CJK, kana, Hangul or emoji measure far narrower than it
/// renders, so shedding concluded a row fitted when it did not and the row
/// wrapped: the exact failure responsive mode exists to prevent.
///
/// This is the practical subset of UAX #11 rather than the whole property,
/// which runs to a few hundred ranges and needs regenerating with every
/// Unicode release. These cover the scripts and emoji anyone realistically has
/// in a directory or session name. A few narrow symbols inside the emoji
/// blocks are counted as wide, which is the safe direction to be wrong in:
/// overestimating sheds one segment too eagerly, underestimating wraps the row.
fn char_width(c: char) -> usize {
    const WIDE: [(u32, u32); 18] = [
        (0x1100, 0x115F),   // Hangul Jamo, initial consonants
        (0x2E80, 0x303E),   // CJK radicals, Kangxi, CJK symbols and punctuation
        (0x3041, 0x33FF),   // kana, Bopomofo, Hangul compatibility, CJK compat
        (0x3400, 0x4DBF),   // CJK unified ideographs extension A
        (0x4E00, 0x9FFF),   // CJK unified ideographs
        (0xA000, 0xA4CF),   // Yi
        (0xA960, 0xA97F),   // Hangul Jamo extended-A
        (0xAC00, 0xD7A3),   // Hangul syllables
        (0xF900, 0xFAFF),   // CJK compatibility ideographs
        (0xFE10, 0xFE19),   // vertical forms
        (0xFE30, 0xFE6F),   // CJK compatibility forms
        (0xFF00, 0xFF60),   // fullwidth forms
        (0xFFE0, 0xFFE6),   // fullwidth signs
        (0x1F300, 0x1F64F), // pictographs and emoticons
        (0x1F680, 0x1F6FF), // transport and map symbols
        (0x1F900, 0x1F9FF), // supplemental symbols and pictographs
        (0x1FA70, 0x1FAFF), // symbols and pictographs extended-A
        (0x20000, 0x3FFFD), // CJK unified ideographs extension B onwards
    ];
    let n = c as u32;
    if WIDE.iter().any(|(lo, hi)| n >= *lo && n <= *hi) {
        2
    } else {
        1
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
            n += char_width(ch);
        }
    }
    n
}

/// Python's round() and the JS shim: half goes to even.
fn round_half_even(x: f64) -> f64 {
    let floor = x.floor();
    let diff = x - floor;
    // Exactly half rounds towards the even neighbour, which is the whole point
    // of this function and was previously spread across four branches, two of
    // which returned the same thing.
    let up = diff > 0.5 || (diff == 0.5 && (floor % 2.0) != 0.0);
    if up {
        floor + 1.0
    } else {
        floor
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

/// A pointer file or a HEAD is a line. A config is a few dozen. Neither is
/// ever large, so a large one is a mistake or a trap, and reading it would only
/// hurt.
pub const SMALL_FILE: u64 = 64 * 1024;
/// Config and the herdr state, which grows with how many workspaces someone
/// has accumulated. Generous rather than tight, since being wrong here costs
/// somebody their settings.
pub const LARGE_FILE: u64 = 4 * 1024 * 1024;
/// The real payload is under a kilobyte. This is far above anything Claude
/// Code would send and far below anything that hurts to allocate.
const MAX_PAYLOAD: u64 = 4 * 1024 * 1024;

/// Reads a file, refusing past `limit` rather than taking however much of
/// someone else's disk is on the other end of the path.
///
/// Nothing here reads a file it chose itself: the config path comes from
/// $HOME, the git files from wherever a .git pointer aims, the herdr state
/// from an env var. Fails closed and prints nothing, the same as every other
/// malformed input.
pub fn read_capped(path: &Path, limit: u64) -> Option<String> {
    // Regular files only. Opening a FIFO blocks until someone writes, and an
    // archive can carry a .git/HEAD that is one. stat does not block.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let mut buf = String::new();
    // limit + 1, so a file exactly at the limit is still readable and one byte
    // over is detectable rather than silently truncated into nonsense.
    file.take(limit + 1).read_to_string(&mut buf).ok()?;
    (buf.len() as u64 <= limit).then_some(buf)
}

/// What .git/HEAD points at.
///
/// Kept as two cases because they mean different things. Folded into one
/// string, a detached HEAD printed as a branch called `a1b2c3d`, in the branch
/// colour, so nothing on the bar said you were detached.
#[derive(Debug, PartialEq)]
enum Head {
    Branch(String),
    /// Short object id.
    Detached(String),
}

impl Head {
    /// The branch name, for collapsing repeats against the directory and
    /// worktree. A detached id is never a name, so it never collapses.
    fn branch_name(&self) -> Option<&str> {
        match self {
            Head::Branch(name) => Some(name),
            Head::Detached(_) => None,
        }
    }

    fn render(&self, ctx: &Ctx) -> String {
        let p = &ctx.cfg.palette;
        match self {
            Head::Branch(name) => ctx.paint(&format!("⎇ {}", name), p.accent_c),
            Head::Detached(id) => ctx.paint(&format!("⎇ @{}", id), p.muted),
        }
    }
}

/// Current HEAD, read straight from .git/HEAD rather than forking git.
fn git_head(cwd: &str) -> Option<Head> {
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
        let pointer = read_capped(&dotgit, SMALL_FILE)?;
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

    let head = read_capped(&dotgit.join("HEAD"), SMALL_FILE)?;
    let head = head.trim();

    const PREFIX: &str = "ref: refs/heads/";
    match head.strip_prefix(PREFIX) {
        Some("") => None,
        Some(rest) => Some(Head::Branch(rest.to_string())),
        // Detached HEAD: a short object id beats showing nothing. Only if it
        // actually looks like one, since a .git file can point anywhere and
        // this would otherwise print the first seven characters of any file
        // named HEAD that it was aimed at.
        None if head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit()) => {
            Some(Head::Detached(head.chars().take(7).collect()))
        }
        None => None,
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
    let head = git_head(cwd);

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

    if let Some(h) = &head {
        let repeats = h
            .branch_name()
            .is_some_and(|b| b == place || Some(b) == tree);
        if !repeats {
            segments.push(h.render(ctx));
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
    let raw = read_capped(
        Path::new(&format!("{home}/.config/herdr/session.json")),
        LARGE_FILE,
    )?;
    let state = json::parse(&raw)?;
    for workspace in state.path("workspaces")?.as_arr()? {
        if workspace.get("id").and_then(Value::as_str) == Some(id.as_str()) {
            return workspace.str_path("custom_name");
        }
    }
    None
}

/// Prefer the herdr workspace label, which is what its tab bar shows.
fn session_label(data: &Value, width: usize, use_herdr: bool) -> Option<String> {
    let herdr = if use_herdr {
        herdr_workspace_name()
    } else {
        None
    };
    let name = herdr.or_else(|| data.str_path("session_name"))?;
    let chars: Vec<char> = name.chars().collect();
    if chars.len() > width {
        let head: String = chars[..width - 1].iter().collect();
        return Some(format!("{}…", head.trim_end()));
    }
    Some(name)
}

/// Wall clock, unless the test harness pins it.
///
/// The golden-file tests need countdowns to render the same bytes on every
/// machine and at every time of day, and the duration boundaries they check
/// (exactly one hour, exactly one day) cannot be hit reliably against a moving
/// clock. The name is deliberately awkward so it cannot be set by accident or
/// mistaken for a feature. Setting it in normal use freezes the countdowns.
fn now_secs() -> f64 {
    if let Ok(pinned) = std::env::var("STATUSLINE_TEST_PINNED_CLOCK") {
        if let Ok(secs) = pinned.parse::<f64>() {
            return secs;
        }
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

fn seg_session(ctx: &Ctx, data: &Value) -> Option<String> {
    let cfg = ctx.cfg;
    session_label(data, cfg.session_name_width, cfg.use_herdr)
        .map(|n| ctx.paint(&n, cfg.palette.brightest))
}

fn seg_model(ctx: &Ctx, data: &Value) -> Option<String> {
    data.path("model.display_name")
        .and_then(Value::as_str)
        .map(|m| ctx.paint(m, ctx.cfg.palette.model))
}

fn seg_location(ctx: &Ctx, data: &Value) -> Option<String> {
    let cwd = data
        .path("workspace.current_dir")
        .and_then(Value::as_str)
        .or_else(|| data.path("cwd").and_then(Value::as_str))
        .unwrap_or("");
    let s = location(ctx, data, cwd);
    (!s.is_empty()).then_some(s)
}

fn seg_pr(ctx: &Ctx, data: &Value) -> Option<String> {
    let num = data.path("pr.number")?;
    if !num.is_truthy() {
        return None;
    }
    let (colour, icon) = pr_state(
        data.path("pr.review_state").and_then(Value::as_str),
        &ctx.cfg.palette,
    );
    // GitLab merge requests are numbered !N. Claude Code marks them kind "mr"
    // and leaves kind out for GitHub.
    let prefix = match data.path("pr.kind").and_then(Value::as_str) {
        Some("mr") => "MR !",
        _ => "PR #",
    };
    let raw = num.as_raw().unwrap_or("");
    let text = if icon.is_empty() {
        format!("{prefix}{raw}")
    } else {
        format!("{prefix}{raw} {icon}")
    };
    Some(ctx.paint(&text, colour))
}

fn seg_modes(ctx: &Ctx, data: &Value) -> Option<String> {
    let p = &ctx.cfg.palette;
    let mut modes: Vec<String> = Vec::new();
    if let Some(effort) = data.path("effort.level").and_then(Value::as_str) {
        modes.push(ctx.paint(effort, p.muted));
    }
    if data
        .path("fast_mode")
        .map(Value::is_truthy)
        .unwrap_or(false)
    {
        modes.push(ctx.paint("fast", p.notice));
    }
    (!modes.is_empty()).then(|| modes.join(" "))
}

/// A percentage fit to print. Infinity and NaN are not percentages, and the
/// ceiling is generous because a switch to a smaller model legitimately exceeds
/// 100. Unbounded, 1e999 rendered as 9223372036854775807%.
fn percentage(v: f64) -> Option<i64> {
    v.is_finite().then(|| v.trunc().clamp(0.0, 999.0) as i64)
}

fn seg_context(ctx: &Ctx, data: &Value) -> Option<String> {
    let cfg = ctx.cfg;
    let pct = data
        .path("context_window.used_percentage")
        .and_then(Value::as_f64)
        .and_then(percentage)?;
    let tip = ctx.fg(ctx.gradient_at(pct as f64 / 100.0));
    let mut out = format!("{} {}{}%{}", ctx.meter(pct, cfg.bar_width), tip, pct, RESET);
    // used_percentage counts input tokens only (input + cache create + cache
    // read), so the token figure uses the same basis.
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
            cfg.palette.muted,
        ));
    }
    Some(out)
}

/// One rate-limit window: label, meter, percentage and countdown to reset.
fn limit_window(ctx: &Ctx, data: &Value, key: &str, label: &str) -> Option<String> {
    let p = &ctx.cfg.palette;
    let window = data.path("rate_limits")?.get(key)?;
    let pct = window
        .get("used_percentage")
        .and_then(Value::as_f64)
        .and_then(percentage)?;
    let tip = ctx.fg(ctx.gradient_at(pct as f64 / 100.0));
    let mut out = format!(
        "{} {} {}{}%{}",
        ctx.paint(label, p.muted),
        ctx.meter(pct, ctx.cfg.limit_bar_width),
        tip,
        pct,
        RESET
    );
    if let Some(resets) = window.get("resets_at").and_then(Value::as_f64) {
        if resets != 0.0 {
            out.push(' ');
            out.push_str(&ctx.paint(&fmt_duration(resets - now_secs()), p.muted));
        }
    }
    Some(out)
}

const FIVE_HOUR: (&str, &str) = ("five_hour", "5h");
const SEVEN_DAY: (&str, &str) = ("seven_day", "7d");

fn limits(ctx: &Ctx, data: &Value, wanted: &[(&str, &str)]) -> Option<String> {
    let parts: Vec<String> = wanted
        .iter()
        .filter_map(|(key, label)| limit_window(ctx, data, key, label))
        .collect();
    (!parts.is_empty()).then(|| parts.join(&ctx.sep))
}

/// Both windows in one segment, matching how they read together.
fn seg_limits(ctx: &Ctx, data: &Value) -> Option<String> {
    limits(ctx, data, &[FIVE_HOUR, SEVEN_DAY])
}

fn seg_limit_5h(ctx: &Ctx, data: &Value) -> Option<String> {
    limits(ctx, data, &[FIVE_HOUR])
}

fn seg_limit_7d(ctx: &Ctx, data: &Value) -> Option<String> {
    limits(ctx, data, &[SEVEN_DAY])
}

fn seg_cost(ctx: &Ctx, data: &Value) -> Option<String> {
    let cost = data.path("cost.total_cost_usd").and_then(Value::as_f64)?;
    (cost != 0.0).then(|| {
        ctx.paint(
            &format!("${}", format_fixed(cost, 2)),
            ctx.cfg.palette.bright,
        )
    })
}

/// Lines Claude has added and removed this session, as `+156 -23`.
///
/// Counts go through fmt_tokens so a long session reads `+12k` rather than
/// growing the row a digit at a time. Hidden until something has changed.
fn seg_lines(ctx: &Ctx, data: &Value) -> Option<String> {
    let count = |key: &str| {
        data.path(key)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite() && *n >= 0.0)
            .unwrap_or(0.0)
    };
    let added = count("cost.total_lines_added");
    let removed = count("cost.total_lines_removed");
    if added == 0.0 && removed == 0.0 {
        return None;
    }
    let p = &ctx.cfg.palette;
    Some(format!(
        "{} {}",
        ctx.paint(&format!("+{}", fmt_tokens(added)), p.good),
        ctx.paint(&format!("-{}", fmt_tokens(removed)), p.bad)
    ))
}

/// Every segment a config file can name, bound to what renders it.
///
/// A table rather than one long match: each renderer is independently
/// readable, and the names live in one place rather than scattered through
/// arms, so `--init` can list them without a second copy going stale.
type Renderer = fn(&Ctx, &Value) -> Option<String>;

const SEGMENTS: [(&str, Renderer); 11] = [
    ("session", seg_session),
    ("model", seg_model),
    ("location", seg_location),
    ("pr", seg_pr),
    ("modes", seg_modes),
    ("context", seg_context),
    ("limits", seg_limits),
    ("limit_5h", seg_limit_5h),
    ("limit_7d", seg_limit_7d),
    ("cost", seg_cost),
    ("lines", seg_lines),
];

pub fn segment_names() -> impl Iterator<Item = &'static str> {
    SEGMENTS.iter().map(|(name, _)| *name)
}

/// Renders one named segment, or None when it has no data to show.
fn segment(name: &str, ctx: &Ctx, data: &Value) -> Option<String> {
    // Unknown names are ignored, so a typo costs a segment and not the bar.
    let (_, render) = SEGMENTS.iter().find(|(n, _)| *n == name)?;
    render(ctx, data)
}

/// Terminal width, from $COLUMNS.
///
/// Claude Code captures stdout rather than attaching it to the terminal, so
/// ioctl and `tput cols` see nothing from in here. It sets COLUMNS and LINES
/// before running the command instead. Requires Claude Code v2.1.153.
fn terminal_width() -> Option<usize> {
    std::env::var("COLUMNS")
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|w| *w > 0)
}

fn joined(parts: &[(String, String)], ctx: &Ctx) -> String {
    parts
        .iter()
        .map(|(_, rendered)| rendered.as_str())
        .collect::<Vec<_>>()
        .join(&ctx.sep)
}

/// Builds a row, shedding segments in `drop_order` while it overflows.
///
/// Without this a long branch name or a narrow split pane wraps the row, which
/// costs a line of conversation and looks broken. Segments not named in
/// drop_order are never shed, so the row degrades to something still useful
/// rather than to nothing.
fn build_line(names: &[String], ctx: &Ctx, data: &Value, budget: Option<usize>) -> String {
    let mut parts: Vec<(String, String)> = names
        .iter()
        .filter_map(|n| segment(n, ctx, data).map(|s| (n.clone(), s)))
        .filter(|(_, s)| !s.is_empty())
        .collect();

    let mut row = joined(&parts, ctx);
    let Some(budget) = budget else { return row };

    // Remeasure only when something actually went. Most candidates in
    // drop_order are not on this row at all, and rebuilding the whole string
    // to discover that a no-op changed nothing is work with no question behind
    // it.
    for candidate in &ctx.cfg.drop_order {
        if display_width(&row) <= budget {
            break;
        }
        let before = parts.len();
        parts.retain(|(name, _)| name != candidate);
        if parts.len() != before {
            row = joined(&parts, ctx);
        }
    }
    row
}

fn detect_truecolor(cfg: &Config) -> bool {
    cfg.truecolor.unwrap_or_else(|| {
        matches!(
            std::env::var("COLORTERM").as_deref(),
            Ok("truecolor") | Ok("24bit")
        )
    })
}

/// The whole bar, or None when there is nothing worth printing.
fn render(cfg: &Config, data: &Value) -> Option<String> {
    let ctx = Ctx::new(cfg, detect_truecolor(cfg));

    let budget = if cfg.responsive {
        terminal_width()
    } else {
        None
    };

    let lines: Vec<String> = [
        build_line(&cfg.line1, &ctx, data, budget),
        build_line(&cfg.line2, &ctx, data, budget),
    ]
    .into_iter()
    .filter(|l| !l.is_empty())
    .collect();
    if lines.is_empty() {
        return None;
    }

    let gap: Vec<String> = match cfg.gap {
        Gap::Off => Vec::new(),
        Gap::Rule => {
            let span = lines.iter().map(|l| display_width(l)).max().unwrap_or(0);
            let width = (round_half_even(span as f64 * cfg.gap_scale) as usize).max(1);
            vec![format!(
                "{}{}{}",
                ctx.fg(cfg.palette.separator),
                cfg.gap_char.repeat(width),
                RESET
            )]
        }
        Gap::Blank => vec![BLANK_ROW.to_string()],
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

    Some(rows.join("\n") + "\n")
}

const HELP: &str = "\
micro-claude-statusline

  (no arguments)      read the Claude Code payload on stdin, print the bar
  --preview [THEME]   render a sample bar, all themes if none is named
  --preview --compact same, in the one-row compact layout
  --init [--force]    write a commented ~/.claude/statusline.json
  --themes            list theme names
  --version           which build this is
  --help              this
";

/// Renders the sample payload under one theme, ignoring the user's config so
/// the comparison is like for like.
fn preview_one(name: &str, data: &Value, preset: Option<&str>, out: &mut String) {
    let mut cfg = Config::default();
    if let Some(p) = preset {
        config::apply_preset(&mut cfg, p);
    }
    cfg.palette = theme::by_name(name);
    // Keep the user's colour depth so the preview matches what they will see.
    cfg.truecolor = Some(detect_truecolor(&Config::default()));
    // A preview is about the theme, not about where the caller happens to be.
    cfg.use_herdr = false;

    out.push_str(&format!("  {name}\n"));
    if let Some(bar) = render(&cfg, data) {
        for line in bar.trim_end().lines() {
            out.push_str(&format!("    {line}\n"));
        }
    }
    out.push('\n');
}

fn preview(requested: Option<&str>, preset: Option<&str>) -> i32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let raw = cli::sample_payload(now);
    let Some(data) = json::parse(raw.trim()) else {
        eprintln!("internal: sample payload did not parse");
        return 1;
    };

    let mut out = String::new();
    match requested {
        Some(name) => {
            if !theme::names().any(|n| n == name) {
                eprintln!("unknown theme: {name}");
                eprintln!(
                    "try one of: {}",
                    theme::names().collect::<Vec<_>>().join(", ")
                );
                return 1;
            }
            preview_one(name, &data, preset, &mut out);
        }
        None => {
            out.push_str("sample data, not your session\n\n");
            for name in theme::names() {
                preview_one(name, &data, preset, &mut out);
            }
        }
    }
    // Written rather than printed: piping to `head` closes the pipe early, and
    // println! panics on EPIPE.
    let _ = std::io::stdout().write_all(out.as_bytes());
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |name: &str| args.iter().any(|a| a == name);

    if flag("--help") || flag("-h") {
        print!("{HELP}");
        return;
    }
    if flag("--version") || flag("-V") {
        println!("micro-claude-statusline {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if flag("--themes") {
        cli::themes();
        return;
    }
    if flag("--init") {
        std::process::exit(cli::init(flag("--force")));
    }
    if flag("--preview") {
        // The value after --preview, if it is not itself a flag.
        let named = args
            .iter()
            .position(|a| a == "--preview")
            .and_then(|i| args.get(i + 1))
            .filter(|a| !a.starts_with('-'))
            .map(String::as_str);
        let preset = flag("--compact").then_some("compact");
        std::process::exit(preview(named, preset));
    }

    let mut raw = String::new();
    // Capped like every other read. Nothing chooses what arrives on stdin, and
    // a payload past this is not one Claude Code sent.
    if std::io::stdin()
        .take(MAX_PAYLOAD + 1)
        .read_to_string(&mut raw)
        .is_err()
        || raw.len() as u64 > MAX_PAYLOAD
    {
        return;
    }

    let data = match json::parse(raw.trim()) {
        Some(v @ Value::Obj(_)) => v,
        // Non-object JSON and malformed input both render nothing.
        _ => return,
    };

    if let Some(bar) = render(&config::load(), &data) {
        let _ = std::io::stdout().write_all(bar.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("statusline-test-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }

        /// A repo at <scratch>/<name> whose HEAD holds `head`.
        fn repo(&self, name: &str, head: &str) -> String {
            let root = self.0.join(name);
            std::fs::create_dir_all(root.join(".git")).unwrap();
            std::fs::write(root.join(".git/HEAD"), head).unwrap();
            root.to_string_lossy().into_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const ID: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn attached_head_is_a_branch() {
        let s = Scratch::new();
        let cwd = s.repo("r", "ref: refs/heads/main\n");
        assert_eq!(git_head(&cwd), Some(Head::Branch("main".into())));
    }

    #[test]
    fn detached_head_is_not_a_branch() {
        let s = Scratch::new();
        let cwd = s.repo("r", &format!("{ID}\n"));
        assert_eq!(git_head(&cwd), Some(Head::Detached("0123456".into())));
    }

    #[test]
    fn head_that_is_neither_shows_nothing() {
        let s = Scratch::new();
        assert_eq!(git_head(&s.repo("words", "not a ref or an id\n")), None);
        assert_eq!(git_head(&s.repo("short", "abc12\n")), None);
        assert_eq!(git_head(&s.repo("empty", "ref: refs/heads/\n")), None);
    }

    #[test]
    fn walks_up_from_a_subdirectory() {
        let s = Scratch::new();
        let root = s.repo("r", "ref: refs/heads/dev\n");
        let sub = Path::new(&root).join("src/deep");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(
            git_head(sub.to_str().unwrap()),
            Some(Head::Branch("dev".into()))
        );
    }

    #[test]
    fn follows_a_worktree_pointer() {
        let s = Scratch::new();
        let main = s.repo("main", "ref: refs/heads/main\n");
        let wt_git = Path::new(&main).join(".git/worktrees/feature");
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
        let wt = s.0.join("feature");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        assert_eq!(
            git_head(wt.to_str().unwrap()),
            Some(Head::Branch("feature".into()))
        );
    }

    #[test]
    fn location_marks_a_detached_head() {
        let s = Scratch::new();
        // Named like the short id, so a detached HEAD that still collapsed
        // against the directory name would vanish from the row.
        let cwd = s.repo("0123456", &format!("{ID}\n"));
        let cfg = Config::default();
        let ctx = Ctx::new(&cfg, true);
        let row = location(&ctx, &Value::Obj(Default::default()), &cwd);
        assert!(row.contains("⎇ @0123456"), "{row:?}");
    }

    #[test]
    fn location_collapses_a_branch_named_like_the_directory() {
        let s = Scratch::new();
        let cwd = s.repo("main", "ref: refs/heads/main\n");
        let cfg = Config::default();
        let ctx = Ctx::new(&cfg, true);
        let row = location(&ctx, &Value::Obj(Default::default()), &cwd);
        assert!(!row.contains('⎇'), "{row:?}");
    }
}
