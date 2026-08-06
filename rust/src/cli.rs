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
  "// preset": "compact puts everything on one row. Omit for the default two.",
  "// note on presets": "a starting point only, keys below still override it",

  "// theme": "{{THEMES}}",
  "theme": "nord",

  "// line1": "identity row. Reorder freely, or move entries to line2.",
  "// segments": "{{SEGMENTS}}",
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

  "// responsive": "shed segments rather than let a row wrap. false keeps them all.",
  "responsive": true,

  "// drop_order": "order segments are shed in when a row will not fit, first to go first",
  "// note on drop_order": "anything left out is never shed, which is why context and location survive",
  "drop_order": ["cost", "modes", "model", "pr", "limits", "session"],

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
    // The lists of themes and segments are filled in from the code that
    // defines them, so a new theme or segment appears here without anyone
    // remembering to update a second copy. Substitution rather than format!,
    // because the template contains a literal "colors": {}.
    let body = DEFAULT_CONFIG
        .replace("{{THEMES}}", &theme::names().collect::<Vec<_>>().join(" | "))
        .replace(
            "{{SEGMENTS}}",
            &crate::segment_names().collect::<Vec<_>>().join(", "),
        );

    match std::fs::write(&path, &body) {
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
    for name in theme::names() {
        println!("{name}");
    }
}
