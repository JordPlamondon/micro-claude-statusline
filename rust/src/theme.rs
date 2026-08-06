//! Named palettes.
//!
//! Every theme fills the same thirteen slots, so swapping one changes the whole
//! bar without touching render code. The meter gradient is derived from four of
//! them rather than defined separately, which keeps a new theme to one row.

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy)]
pub struct Palette {
    /// Separators and the rule between rows. Should recede.
    pub separator: Rgb,
    /// Unfilled meter track, and PR "draft". Dimmest readable tone.
    pub dim: Rgb,
    /// Secondary text that still needs reading: token counts, countdowns, effort.
    pub muted: Rgb,
    /// Session cost.
    pub bright: Rgb,
    /// Session or workspace name. The brightest thing on the bar.
    pub brightest: Rgb,
    /// Worktree marker.
    pub accent_a: Rgb,
    /// Directory name, and the low end of the meter gradient.
    pub accent_b: Rgb,
    /// Git branch.
    pub accent_c: Rgb,
    /// Model name.
    pub model: Rgb,
    /// PR approved, and the second gradient stop.
    pub good: Rgb,
    /// PR pending, and the third gradient stop.
    pub warn: Rgb,
    /// PR changes requested, and the top of the gradient.
    pub bad: Rgb,
    /// Fast mode.
    pub notice: Rgb,
}

/// Every slot a config file can override, bound to the field it sets.
///
/// These names appear in the config, in the `--init` template and in
/// CONFIG.md. Binding them to the fields here means the parser cannot drift
/// from the struct: adding a field and forgetting to extend the parser used to
/// mean a slot that documented itself and then quietly did nothing.
type SetSlot = fn(&mut Palette, Rgb);

pub const SLOTS: [(&str, SetSlot); 13] = [
    ("separator", |p, c| p.separator = c),
    ("dim", |p, c| p.dim = c),
    ("muted", |p, c| p.muted = c),
    ("bright", |p, c| p.bright = c),
    ("brightest", |p, c| p.brightest = c),
    ("accent_a", |p, c| p.accent_a = c),
    ("accent_b", |p, c| p.accent_b = c),
    ("accent_c", |p, c| p.accent_c = c),
    ("model", |p, c| p.model = c),
    ("good", |p, c| p.good = c),
    ("warn", |p, c| p.warn = c),
    ("bad", |p, c| p.bad = c),
    ("notice", |p, c| p.notice = c),
];

pub fn slot_names() -> impl Iterator<Item = &'static str> {
    SLOTS.iter().map(|(name, _)| *name)
}

impl Palette {
    /// Meter stops, derived so a new theme does not have to restate them.
    pub fn gradient(&self) -> [(f64, Rgb); 4] {
        [
            (0.00, self.accent_b),
            (0.40, self.good),
            (0.70, self.warn),
            (1.00, self.bad),
        ]
    }
}

const NORD: Palette = Palette {
    separator: (0x43, 0x4C, 0x5E),
    dim: (0x4C, 0x56, 0x6A),
    muted: (0x7D, 0x86, 0x96),
    bright: (0xD8, 0xDE, 0xE9),
    brightest: (0xEC, 0xEF, 0xF4),
    accent_a: (0x8F, 0xBC, 0xBB),
    accent_b: (0x88, 0xC0, 0xD0),
    accent_c: (0x81, 0xA1, 0xC1),
    model: (0xB4, 0x8E, 0xAD),
    good: (0xA3, 0xBE, 0x8C),
    warn: (0xEB, 0xCB, 0x8B),
    bad: (0xBF, 0x61, 0x6A),
    notice: (0xD0, 0x87, 0x70),
};

const CATPPUCCIN: Palette = Palette {
    separator: (0x45, 0x47, 0x5A),
    dim: (0x58, 0x5B, 0x70),
    muted: (0x7F, 0x84, 0x9C),
    bright: (0xBA, 0xC2, 0xDE),
    brightest: (0xCD, 0xD6, 0xF4),
    accent_a: (0x94, 0xE2, 0xD5),
    accent_b: (0x89, 0xDC, 0xEB),
    accent_c: (0x89, 0xB4, 0xFA),
    model: (0xCB, 0xA6, 0xF7),
    good: (0xA6, 0xE3, 0xA1),
    warn: (0xF9, 0xE2, 0xAF),
    bad: (0xF3, 0x8B, 0xA8),
    notice: (0xFA, 0xB3, 0x87),
};

const TOKYO_NIGHT: Palette = Palette {
    separator: (0x29, 0x2E, 0x42),
    dim: (0x41, 0x48, 0x68),
    muted: (0x56, 0x5F, 0x89),
    bright: (0xA9, 0xB1, 0xD6),
    brightest: (0xC0, 0xCA, 0xF5),
    accent_a: (0x1A, 0xBC, 0x9C),
    accent_b: (0x7D, 0xCF, 0xFF),
    accent_c: (0x7A, 0xA2, 0xF7),
    model: (0xBB, 0x9A, 0xF7),
    good: (0x9E, 0xCE, 0x6A),
    warn: (0xE0, 0xAF, 0x68),
    bad: (0xF7, 0x76, 0x8E),
    notice: (0xFF, 0x9E, 0x64),
};

const GRUVBOX: Palette = Palette {
    separator: (0x3C, 0x38, 0x36),
    dim: (0x50, 0x49, 0x45),
    muted: (0x92, 0x83, 0x74),
    bright: (0xD5, 0xC4, 0xA1),
    brightest: (0xEB, 0xDB, 0xB2),
    accent_a: (0x8E, 0xC0, 0x7C),
    accent_b: (0x83, 0xA5, 0x98),
    accent_c: (0x45, 0x85, 0x88),
    model: (0xD3, 0x86, 0x9B),
    good: (0xB8, 0xBB, 0x26),
    warn: (0xFA, 0xBD, 0x2F),
    bad: (0xFB, 0x49, 0x34),
    notice: (0xFE, 0x80, 0x19),
};

const DRACULA: Palette = Palette {
    separator: (0x44, 0x47, 0x5A),
    dim: (0x62, 0x72, 0xA4),
    muted: (0x8A, 0x93, 0xB8),
    bright: (0xE0, 0xE0, 0xDC),
    brightest: (0xF8, 0xF8, 0xF2),
    accent_a: (0x8B, 0xE9, 0xFD),
    accent_b: (0x8B, 0xE9, 0xFD),
    accent_c: (0xBD, 0x93, 0xF9),
    model: (0xFF, 0x79, 0xC6),
    good: (0x50, 0xFA, 0x7B),
    warn: (0xF1, 0xFA, 0x8C),
    bad: (0xFF, 0x55, 0x55),
    notice: (0xFF, 0xB8, 0x6C),
};

const ONE_DARK: Palette = Palette {
    separator: (0x3E, 0x44, 0x51),
    dim: (0x4B, 0x52, 0x63),
    muted: (0x82, 0x89, 0x97),
    bright: (0xAB, 0xB2, 0xBF),
    brightest: (0xDC, 0xDF, 0xE4),
    accent_a: (0x56, 0xB6, 0xC2),
    accent_b: (0x61, 0xAF, 0xEF),
    accent_c: (0x52, 0x8B, 0xFF),
    model: (0xC6, 0x78, 0xDD),
    good: (0x98, 0xC3, 0x79),
    warn: (0xE5, 0xC0, 0x7B),
    bad: (0xE0, 0x6C, 0x75),
    notice: (0xD1, 0x9A, 0x66),
};

/// Every theme, in the order `--themes` and `--preview` list them.
///
/// The one place a name is bound to a palette. The list and the lookup used to
/// be separate, so adding a theme meant editing both, and disagreeing between
/// them failed quietly: `--themes` would offer a name that the lookup resolved
/// to nord, and the user would pick it and see the wrong colours.
pub const THEMES: [(&str, Palette); 6] = [
    ("nord", NORD),
    ("catppuccin", CATPPUCCIN),
    ("tokyo-night", TOKYO_NIGHT),
    ("gruvbox", GRUVBOX),
    ("dracula", DRACULA),
    ("one-dark", ONE_DARK),
];

pub fn names() -> impl Iterator<Item = &'static str> {
    THEMES.iter().map(|(name, _)| *name)
}

/// Unknown names fall back to nord rather than failing, so a typo in the config
/// costs a colour scheme instead of the whole status line.
pub fn by_name(name: &str) -> Palette {
    THEMES
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(NORD, |(_, palette)| *palette)
}
