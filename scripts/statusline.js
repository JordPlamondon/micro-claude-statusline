#!/usr/bin/env bun
// Claude Code status line, Nord palette.
//
// Line 1: session name, model, project, branch/worktree, open PR, effort mode
// Line 2: context usage, 5h + 7d rate limits, session cost
//
// Ported from statusline.py. Bun starts in ~40 ms against Python's ~270 ms, and
// the status line re-runs on every event plus every refreshInterval tick in
// every open session, so interpreter startup dominated the cost.

// No node-compat imports. `fs` alone costs 6-16 ms to load, which was the single
// largest cost in this script; `path` and `os` are replaced by string operations
// and process.env. Bun's own file API needs no import.

const HOME = process.env.HOME || "";

// Minimal path helpers. Inputs are absolute paths from Claude Code, so these do
// not need to handle the general cases the node module does.
const stripSlashes = (p) => p.replace(/\/+$/, "");
const baseName = (p) => {
  const s = stripSlashes(p);
  return s.slice(s.lastIndexOf("/") + 1);
};
const dirName = (p) => {
  const s = stripSlashes(p);
  const i = s.lastIndexOf("/");
  return i <= 0 ? "/" : s.slice(0, i);
};

async function readText(path) {
  try {
    return await Bun.file(path).text();
  } catch {
    return null;
  }
}

const ANSI_RE = /\x1b\[[0-9;]*m/g;
const RESET = "\x1b[0m";

// Nord palette, https://www.nordtheme.com/docs/colors-and-palettes
const NORD = {
  polar2: [0x43, 0x4c, 0x5e],
  polar3: [0x4c, 0x56, 0x6a],
  snow4: [0xd8, 0xde, 0xe9],
  snow6: [0xec, 0xef, 0xf4],
  // Not a Nord token. Sits between polar3 and snow4, which have a large gap
  // with nothing between them, for secondary text that still needs reading.
  muted: [0x7d, 0x86, 0x96],
  frost7: [0x8f, 0xbc, 0xbb],
  frost8: [0x88, 0xc0, 0xd0],
  frost9: [0x81, 0xa1, 0xc1],
  red: [0xbf, 0x61, 0x6a],
  orange: [0xd0, 0x87, 0x70],
  yellow: [0xeb, 0xcb, 0x8b],
  green: [0xa3, 0xbe, 0x8c],
  purple: [0xb4, 0x8e, 0xad],
};

// Meter gradient: position along the bar maps to colour.
const GRADIENT = [
  [0.0, NORD.frost8],
  [0.4, NORD.green],
  [0.7, NORD.yellow],
  [1.0, NORD.red],
];

const EIGHTHS = " ▏▎▍▌▋▊▉█";
const TRACK = "░";

const BAR_WIDTH = 16;
const LIMIT_BAR_WIDTH = 6;

// Session name, truncated. AI-generated titles run long, and this leads the row.
const SESSION_NAME_WIDTH = 24;

// herdr writes live workspace state here, including the label shown in its tab
// bar. Reading it costs a fraction of a ms, against ~16 ms to ask the server.
const HERDR_SESSION = `${HOME}/.config/herdr/session.json`;

// U+2800 renders blank but is not whitespace, so it survives trimming.
const BLANK_ROW = "⠀";

// Gap between the identity and usage rows. A terminal row is the smallest unit,
// so there is no half-height gap; a faint rule reads smaller than empty space.
//   "rule"  — dim dotted line, the tightest-looking gap
//   "blank" — fully empty row, the largest gap
//   "none"  — rows touch
const GAP_STYLE = "rule";

// Rule character, and its width as a fraction of the widest row. 1.0 spans fully.
const GAP_RULE_CHAR = "┈";
const GAP_RULE_SCALE = 1.0;

// Blank rows printed below the bar, to keep it off the mode indicator. 0 disables.
const BOTTOM_SPACING = 1;

const PR_STATE = {
  approved: ["green", "✓"],
  changes_requested: ["red", "✗"],
  pending: ["yellow", "●"],
  draft: ["polar3", "○"],
};

// Nearest xterm-256 index for an RGB triple: the 6x6x6 cube, or the grey ramp
// when the channels are close enough to neutral for grey to be a better match.
const CUBE = [0, 95, 135, 175, 215, 255];
function to256(rgb) {
  const [r, g, b] = rgb;
  const axis = (v) =>
    CUBE.reduce((best, l, i) => (Math.abs(v - l) < Math.abs(v - CUBE[best]) ? i : best), 0);
  const [ri, gi, bi] = [axis(r), axis(g), axis(b)];
  const sq = (x) => x * x;
  const cubeErr = sq(r - CUBE[ri]) + sq(g - CUBE[gi]) + sq(b - CUBE[bi]);

  // Grey ramp is 232..255 at 8 + 10n.
  const avg = Math.floor((r + g + b) / 3);
  const greyI = Math.min(23, Math.max(0, Math.round((avg - 8) / 10)));
  const greyV = 8 + greyI * 10;
  const greyErr = sq(r - greyV) + sq(g - greyV) + sq(b - greyV);

  return greyErr < cubeErr ? 232 + greyI : 16 + 36 * ri + 6 * gi + bi;
}

// True when the terminal advertises 24-bit colour. Everything else gets the
// 256-colour approximation, which every terminal worth using supports.
const TRUECOLOR = ["truecolor", "24bit"].includes(process.env.COLORTERM);

const fg = (rgb) =>
  TRUECOLOR
    ? `\x1b[38;2;${rgb[0]};${rgb[1]};${rgb[2]}m`
    : `\x1b[38;5;${to256(rgb)}m`;

// Visible column count, ignoring ANSI colour escapes. Spread rather than
// .length so astral characters count once, matching Python's len() on str.
const displayWidth = (text) => [...text.replace(ANSI_RE, "")].length;

const paint = (text, key) => `${fg(NORD[key])}${text}${RESET}`;

// Python's round() is banker's rounding; JS Math.round() is half-up. They differ
// on exact .5, which the eighth-block maths hits, so match Python explicitly.
function roundHalfEven(x) {
  const floor = Math.floor(x);
  const diff = x - floor;
  if (diff > 0.5) return floor + 1;
  if (diff < 0.5) return floor;
  return floor % 2 === 0 ? floor : floor + 1;
}

async function readJson(path) {
  const text = await readText(path);
  if (text === null) return null;
  try {
    return JSON.parse(text);
  } catch {
    // Caught mid-write by whoever owns the file.
    return null;
  }
}

function gradientAt(t) {
  t = Math.max(0, Math.min(1, t));
  for (let i = 0; i < GRADIENT.length - 1; i++) {
    const [t0, c0] = GRADIENT[i];
    const [t1, c1] = GRADIENT[i + 1];
    if (t0 <= t && t <= t1) {
      const span = t1 - t0;
      const f = span === 0 ? 0 : (t - t0) / span;
      return [0, 1, 2].map((j) => roundHalfEven(c0[j] + (c1[j] - c0[j]) * f));
    }
  }
  return GRADIENT[GRADIENT.length - 1][1];
}

// Gradient bar accurate to an eighth of a cell.
function meter(pct, width) {
  const eighths = Math.max(
    0,
    Math.min(width * 8, roundHalfEven((pct / 100) * width * 8)),
  );
  const full = Math.floor(eighths / 8);
  const remainder = eighths % 8;

  const cells = [];
  for (let i = 0; i < full; i++) {
    cells.push(`${fg(gradientAt((i + 0.5) / width))}█`);
  }
  let filled = full;

  if (remainder && full < width) {
    cells.push(`${fg(gradientAt((full + 0.5) / width))}${EIGHTHS[remainder]}`);
    filled += 1;
  }

  if (filled < width) {
    cells.push(`${fg(NORD.polar3)}${TRACK.repeat(width - filled)}`);
  }

  return cells.join("") + RESET;
}

function fmtTokens(n) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`.replace(".0M", "M");
  if (n >= 1_000) return `${roundHalfEven(n / 1_000)}k`;
  return String(n);
}

// Countdown whose precision matches its horizon.
//
// Minutes matter on the 5h window, where you might wait it out. On the 7d
// window they are noise, and hours alone force a mental divide by 24.
// Spaces between units because "121h25m" runs together at a glance.
function fmtDuration(seconds) {
  if (seconds <= 0) return "now";
  let minutes = Math.floor(seconds / 60);
  let hours = Math.floor(minutes / 60);
  minutes %= 60;
  const days = Math.floor(hours / 24);
  hours %= 24;
  if (days) return `${days}d ${hours}h`;
  if (hours) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${minutes}m`;
}

// Current branch, read straight from .git/HEAD.
//
// Forking `git` dominated the original script's runtime, and the status line
// re-renders constantly. Two file reads cost a fraction of that, and unlike a
// cache they can never go stale.
async function gitBranch(cwd) {
  if (!cwd || !cwd.startsWith("/")) return null;
  let path = stripSlashes(cwd) || "/";

  // The cwd must itself be a directory. Without this a stale or mistyped path
  // walks up and reports an ancestor's branch as though it were this one.
  // EISDIR is the success case here: reading a directory as text always fails.
  try {
    await Bun.file(path).text();
    return null; // readable as text, so it is a file rather than a directory
  } catch (err) {
    if (err.code !== "EISDIR") return null; // ENOENT, EACCES, and so on
  }

  // Walk up to the repo, reading .git rather than stat-ing it: EISDIR means a
  // normal repo, a successful read means a worktree pointer, ENOENT means keep
  // going. That distinction is the only thing statSync was needed for.
  let dotgit = null;
  for (;;) {
    const candidate = `${path}/.git`;
    let pointer = null;
    try {
      pointer = await Bun.file(candidate).text();
    } catch (err) {
      if (err.code === "EISDIR") {
        dotgit = candidate;
        break;
      }
      // ENOENT, EACCES and friends: not a repo at this level.
      const parent = dirName(path);
      if (parent === path) return null;
      path = parent;
      continue;
    }
    // Linked worktree: "gitdir: /abs/path/.git/worktrees/<name>"
    pointer = pointer.trim();
    if (!pointer.startsWith("gitdir:")) return null;
    dotgit = pointer.slice(pointer.indexOf(":") + 1).trim();
    if (!dotgit.startsWith("/")) dotgit = `${path}/${dotgit}`;
    break;
  }

  const head = await readText(`${dotgit}/HEAD`);
  if (head === null) return null;

  const trimmed = head.trim();
  const prefix = "ref: refs/heads/";
  if (trimmed.startsWith(prefix)) return trimmed.slice(prefix.length) || null;
  // Detached HEAD, mid-rebase or on a tag: a short SHA beats showing nothing.
  return trimmed.slice(0, 7) || null;
}

// Directory, worktree and branch, with repeated names collapsed.
//
// Worktrees are commonly created with the directory, worktree and branch all
// sharing one name, which would otherwise print that name three times.
async function location(data, cwd) {
  const place = baseName(cwd);
  // git_worktree lives under workspace; worktree.name is top level.
  const tree =
    (data.workspace || {}).git_worktree || (data.worktree || {}).name || null;
  const branch = await gitBranch(cwd);

  let segments;
  if (tree && tree === place) {
    // Directory is the worktree; mark it once instead of naming it twice.
    segments = [paint(`⑂ ${place}`, "frost8")];
  } else {
    segments = place ? [paint(place, "frost8")] : [];
    if (tree) segments.push(paint(`⑂ ${tree}`, "frost7"));
  }

  if (branch && branch !== place && branch !== tree) {
    segments.push(paint(`⎇ ${branch}`, "frost9"));
  }

  return segments.join(" ");
}

// Label of the herdr workspace this pane belongs to, if it has one.
//
// Only custom labels are returned. herdr falls back to the directory basename
// for unlabelled workspaces, which the location segment already shows.
async function herdrWorkspaceName() {
  const id = process.env.HERDR_WORKSPACE_ID;
  if (!id) return null;
  const state = await readJson(HERDR_SESSION);
  if (!state) return null;
  for (const workspace of state.workspaces || []) {
    if (workspace.id === id) {
      return (workspace.custom_name || "").trim() || null;
    }
  }
  return null;
}

// Prefer the herdr workspace label, which is what the tab bar shows.
async function sessionLabel(data) {
  let name = (await herdrWorkspaceName()) || (data.session_name || "").trim();
  if (!name) return null;
  if ([...name].length > SESSION_NAME_WIDTH) {
    name = [...name].slice(0, SESSION_NAME_WIDTH - 1).join("").trimEnd() + "…";
  }
  return name;
}

const joinParts = (parts) =>
  parts.filter(Boolean).join(` ${fg(NORD.polar2)}│${RESET} `);

async function identityLine(data) {
  const parts = [];

  // Leads the row: it is the only segment that differs between panes on the
  // same project, so it is what the eye needs first when scanning tabs.
  const name = await sessionLabel(data);
  if (name) parts.push(paint(name, "snow6"));

  const model = (data.model || {}).display_name;
  if (model) parts.push(paint(model, "purple"));

  const workspace = data.workspace || {};
  const cwd = workspace.current_dir || data.cwd || "";

  parts.push(await location(data, cwd));

  const pr = data.pr || {};
  if (pr.number) {
    const [key, icon] = PR_STATE[pr.review_state] || ["polar3", ""];
    parts.push(paint(`PR #${pr.number}${icon ? " " + icon : ""}`, key));
  }

  const modes = [];
  const effort = (data.effort || {}).level;
  if (effort) modes.push(paint(effort, "muted"));
  if (data.fast_mode) modes.push(paint("fast", "orange"));
  if (modes.length) parts.push(modes.join(" "));

  return joinParts(parts);
}

function usageLine(data) {
  const parts = [];

  const ctx = data.context_window || {};
  if (ctx.used_percentage !== undefined && ctx.used_percentage !== null) {
    const pct = Math.trunc(ctx.used_percentage);
    const tip = fg(gradientAt(pct / 100));
    let segment = `${meter(pct, BAR_WIDTH)} ${tip}${pct}%${RESET}`;
    // used_percentage counts input tokens only (input + cache create + cache
    // read), so the token figure uses the same basis rather than adding output.
    const used = ctx.total_input_tokens || 0;
    const size = ctx.context_window_size;
    if (used && size) {
      segment += " " + paint(`${fmtTokens(used)}/${fmtTokens(size)}`, "muted");
    }
    parts.push(segment);
  }

  const limits = data.rate_limits || {};
  for (const [key, label] of [
    ["five_hour", "5h"],
    ["seven_day", "7d"],
  ]) {
    const window = limits[key] || {};
    if (window.used_percentage === undefined || window.used_percentage === null) {
      continue;
    }
    const pct = Math.trunc(window.used_percentage);
    const tip = fg(gradientAt(pct / 100));
    let segment =
      `${paint(label, "muted")} ${meter(pct, LIMIT_BAR_WIDTH)} ` +
      `${tip}${pct}%${RESET}`;
    if (window.resets_at) {
      segment +=
        " " + paint(fmtDuration(window.resets_at - Date.now() / 1000), "muted");
    }
    parts.push(segment);
  }

  const cost = (data.cost || {}).total_cost_usd;
  if (cost) parts.push(paint(`$${cost.toFixed(2)}`, "snow4"));

  return joinParts(parts);
}

async function main(raw) {
  let data;
  try {
    data = JSON.parse(raw);
  } catch {
    return;
  }
  if (data === null || typeof data !== "object") return;

  const lines = [await identityLine(data), usageLine(data)].filter(Boolean);
  if (!lines.length) return;

  let gap;
  if (GAP_STYLE === "none") {
    gap = [];
  } else if (GAP_STYLE === "rule") {
    const span = Math.max(...lines.map(displayWidth));
    const width = Math.max(1, roundHalfEven(span * GAP_RULE_SCALE));
    gap = [`${fg(NORD.polar2)}${GAP_RULE_CHAR.repeat(width)}${RESET}`];
  } else {
    gap = [BLANK_ROW];
  }

  const rows = [];
  for (const line of lines) {
    if (rows.length) rows.push(...gap);
    rows.push(line);
  }
  for (let i = 0; i < BOTTOM_SPACING; i++) rows.push(BLANK_ROW);

  process.stdout.write(rows.join("\n") + "\n");
}

await main(await Bun.stdin.text());
