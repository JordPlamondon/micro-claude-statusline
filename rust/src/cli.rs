//! Subcommands that are not the status line itself.
//!
//! These exist because nobody reads a README to discover that `gap_scale` is a
//! setting, and because picking a theme by imagining it does not work.

use crate::theme;

/// A representative payload: every segment populated, nothing at a boundary.
/// `resets_at` is filled in at call time so the countdowns look plausible.
pub fn sample_payload(now: i64) -> String {
    format!(
        r#"{{"session_name":"promo-combine",
             "model":{{"display_name":"Opus 5 (1M context)"}},
             "workspace":{{"current_dir":"/home/you/projects/acme-site"}},
             "context_window":{{"used_percentage":42,"total_input_tokens":424000,
                               "context_window_size":1000000}},
             "cost":{{"total_cost_usd":5.72}},
             "effort":{{"level":"high"}},
             "pr":{{"number":265,"review_state":"approved"}},
             "rate_limits":{{"five_hour":{{"used_percentage":38,"resets_at":{}}},
                            "seven_day":{{"used_percentage":61,"resets_at":{}}}}}}}"#,
        now + 8400,
        now + 437_400
    )
}

pub const DEFAULT_CONFIG: &str = r#"{
  "// theme": "nord | catppuccin | tokyo-night | gruvbox | dracula | one-dark",
  "theme": "nord",

  "// line1": "identity row. Reorder freely, or move entries to line2.",
  "// segments": "session, model, location, pr, modes, context, limits, limit_5h, limit_7d, cost",
  "line1": ["session", "model", "location", "pr", "modes"],

  "// line2": "usage row. Drop a segment by leaving it out.",
  "line2": ["context", "limits", "cost"],

  "// bar_width": "context meter width in columns",
  "bar_width": 16,

  "// limit_bar_width": "width of each rate-limit meter",
  "limit_bar_width": 6,

  "// session_name_width": "truncate the leading label past this many characters",
  "session_name_width": 24,

  "// gap": "rule | blank | none, between the two rows",
  "gap": "rule",

  "// gap_char": "character the rule is drawn with",
  "gap_char": "┈",

  "// gap_scale": "rule width as a fraction of the widest row, 0 to 1",
  "gap_scale": 1.0,

  "// bottom_spacing": "blank rows below the bar, to clear the mode indicator",
  "bottom_spacing": 1,

  "// colors": "override individual slots on top of the theme. Hex or [r,g,b].",
  "// slots": "separator, dim, muted, bright, brightest, accent_a, accent_b, accent_c, model, good, warn, bad, notice",
  "colors": {},

  "// truecolor": "true or false to force, omit to detect from COLORTERM",
  "// note": "keys beginning with // are ignored, they are notes to yourself"
}
"#;

/// Writes the commented default config, refusing to clobber an existing one.
pub fn init(force: bool) -> i32 {
    let Ok(home) = std::env::var("HOME") else {
        eprintln!("HOME is not set");
        return 1;
    };
    let path = format!("{home}/.claude/statusline.json");

    if !force && std::path::Path::new(&path).exists() {
        eprintln!("{path} already exists. Pass --force to overwrite it.");
        return 1;
    }
    if let Some(dir) = std::path::Path::new(&path).parent() {
        if std::fs::create_dir_all(dir).is_err() {
            eprintln!("could not create {}", dir.display());
            return 1;
        }
    }
    match std::fs::write(&path, DEFAULT_CONFIG) {
        Ok(_) => {
            println!("wrote {path}");
            println!("every value there is already the default, so edit freely");
            0
        }
        Err(e) => {
            eprintln!("could not write {path}: {e}");
            1
        }
    }
}

pub fn themes() {
    for name in theme::NAMES {
        println!("{name}");
    }
}
