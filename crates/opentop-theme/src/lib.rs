//! opentop theme system.
//!
//! A [`Theme`] assigns semantic color roles (cpu, memory, net_rx, ok/warn/crit,
//! gradients, …) so widgets never hardcode colors. Two themes ship built in —
//! [`Theme::dark`] and [`Theme::light`] — and any theme can be customized
//! from TOML via [`Theme::from_toml`], where every field is optional and
//! falls back to the dark theme.

use std::str::FromStr;

use ratatui::style::Color;
use serde::Deserialize;

/// Semantic color roles used by every widget.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: String,
    /// Panel background.
    pub bg: Color,
    /// Primary text.
    pub fg: Color,
    /// Secondary text, hints, unavailable states.
    pub muted: Color,
    /// Highlights, links, active elements.
    pub accent: Color,
    /// Widget borders.
    pub border: Color,
    /// Panel titles.
    pub title: Color,
    /// Healthy / good state.
    pub ok: Color,
    /// Warning state.
    pub warn: Color,
    /// Critical / failing state.
    pub crit: Color,
    /// CPU series.
    pub cpu: Color,
    /// Memory series.
    pub memory: Color,
    /// Disk series.
    pub disk: Color,
    /// Network receive series.
    pub net_rx: Color,
    /// Network transmit series.
    pub net_tx: Color,
    /// Temperature gradient: cool, warm, hot.
    pub temp: [Color; 3],
    /// Usage gradient: low, mid, high.
    pub usage: [Color; 3],
    /// Unfilled gauge background.
    pub gauge_bg: Color,
    /// Selected row background.
    pub selection_bg: Color,
    /// Selected row foreground.
    pub selection_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Built-in theme names, for `--theme` flags and config files.
pub const BUILTIN_THEMES: [&str; 2] = ["dark", "light"];

impl Theme {
    /// Built-in dark theme (GitHub-dark inspired).
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),
            bg: Color::Rgb(0x0e, 0x11, 0x16),
            fg: Color::Rgb(0xc9, 0xd1, 0xd9),
            muted: Color::Rgb(0x6e, 0x76, 0x81),
            accent: Color::Rgb(0x58, 0xa6, 0xff),
            border: Color::Rgb(0x30, 0x36, 0x3d),
            title: Color::Rgb(0xe6, 0xed, 0xf3),
            ok: Color::Rgb(0x3f, 0xb9, 0x50),
            warn: Color::Rgb(0xd2, 0x99, 0x22),
            crit: Color::Rgb(0xf8, 0x51, 0x49),
            cpu: Color::Rgb(0x58, 0xa6, 0xff),
            memory: Color::Rgb(0xbc, 0x8c, 0xff),
            disk: Color::Rgb(0x79, 0xc0, 0xff),
            net_rx: Color::Rgb(0x3f, 0xb9, 0x50),
            net_tx: Color::Rgb(0xd2, 0x99, 0x22),
            temp: [
                Color::Rgb(0x3f, 0xb9, 0x50),
                Color::Rgb(0xd2, 0x99, 0x22),
                Color::Rgb(0xf8, 0x51, 0x49),
            ],
            usage: [
                Color::Rgb(0x3f, 0xb9, 0x50),
                Color::Rgb(0xd2, 0x99, 0x22),
                Color::Rgb(0xf8, 0x51, 0x49),
            ],
            gauge_bg: Color::Rgb(0x21, 0x26, 0x2d),
            selection_bg: Color::Rgb(0x1f, 0x6f, 0xeb),
            selection_fg: Color::Rgb(0xff, 0xff, 0xff),
        }
    }

    /// Built-in light theme (GitHub-light inspired).
    pub fn light() -> Self {
        Self {
            name: "light".into(),
            bg: Color::Rgb(0xff, 0xff, 0xff),
            fg: Color::Rgb(0x24, 0x29, 0x2f),
            muted: Color::Rgb(0x57, 0x60, 0x6a),
            accent: Color::Rgb(0x09, 0x69, 0xda),
            border: Color::Rgb(0xd0, 0xd7, 0xde),
            title: Color::Rgb(0x1f, 0x23, 0x28),
            ok: Color::Rgb(0x1a, 0x7f, 0x37),
            warn: Color::Rgb(0x9a, 0x67, 0x00),
            crit: Color::Rgb(0xcf, 0x22, 0x2e),
            cpu: Color::Rgb(0x09, 0x69, 0xda),
            memory: Color::Rgb(0x82, 0x50, 0xdf),
            disk: Color::Rgb(0x0a, 0x30, 0x69),
            net_rx: Color::Rgb(0x1a, 0x7f, 0x37),
            net_tx: Color::Rgb(0xbc, 0x4c, 0x00),
            temp: [
                Color::Rgb(0x1a, 0x7f, 0x37),
                Color::Rgb(0x9a, 0x67, 0x00),
                Color::Rgb(0xcf, 0x22, 0x2e),
            ],
            usage: [
                Color::Rgb(0x1a, 0x7f, 0x37),
                Color::Rgb(0x9a, 0x67, 0x00),
                Color::Rgb(0xcf, 0x22, 0x2e),
            ],
            gauge_bg: Color::Rgb(0xea, 0xee, 0xf2),
            selection_bg: Color::Rgb(0x09, 0x69, 0xda),
            selection_fg: Color::Rgb(0xff, 0xff, 0xff),
        }
    }

    /// Built-in theme by name (case-insensitive): "dark" or "light".
    pub fn builtin(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "dark" => Some(Self::dark()),
            "light" => Some(Self::light()),
            _ => None,
        }
    }

    /// Parse a (partial) theme from TOML. Missing fields fall back to the
    /// dark theme, so a config only needs the colors it wants to change.
    ///
    /// ```toml
    /// base = "light"
    /// accent = "#ff8800"
    /// net_rx = "green"
    /// ```
    pub fn from_toml(source: &str) -> Result<Self, ThemeError> {
        let raw: ThemeToml = toml::from_str(source).map_err(ThemeError::Toml)?;
        let mut theme = match raw.base.as_deref() {
            Some(base) => Self::builtin(base).ok_or_else(|| ThemeError::UnknownBase {
                name: base.to_string(),
            })?,
            None => Self::dark(),
        };

        fn apply(target: &mut Color, value: &Option<ColorDef>) {
            if let Some(ColorDef(color)) = value {
                *target = *color;
            }
        }
        if let Some(name) = raw.name {
            theme.name = name;
        }
        apply(&mut theme.bg, &raw.bg);
        apply(&mut theme.fg, &raw.fg);
        apply(&mut theme.muted, &raw.muted);
        apply(&mut theme.accent, &raw.accent);
        apply(&mut theme.border, &raw.border);
        apply(&mut theme.title, &raw.title);
        apply(&mut theme.ok, &raw.ok);
        apply(&mut theme.warn, &raw.warn);
        apply(&mut theme.crit, &raw.crit);
        apply(&mut theme.cpu, &raw.cpu);
        apply(&mut theme.memory, &raw.memory);
        apply(&mut theme.disk, &raw.disk);
        apply(&mut theme.net_rx, &raw.net_rx);
        apply(&mut theme.net_tx, &raw.net_tx);
        if let Some(ColorDef(color)) = raw.temp_low {
            theme.temp[0] = color;
        }
        if let Some(ColorDef(color)) = raw.temp_mid {
            theme.temp[1] = color;
        }
        if let Some(ColorDef(color)) = raw.temp_high {
            theme.temp[2] = color;
        }
        if let Some(ColorDef(color)) = raw.usage_low {
            theme.usage[0] = color;
        }
        if let Some(ColorDef(color)) = raw.usage_mid {
            theme.usage[1] = color;
        }
        if let Some(ColorDef(color)) = raw.usage_high {
            theme.usage[2] = color;
        }
        apply(&mut theme.gauge_bg, &raw.gauge_bg);
        apply(&mut theme.selection_bg, &raw.selection_bg);
        apply(&mut theme.selection_fg, &raw.selection_fg);

        Ok(theme)
    }

    /// Gradient color for a temperature reading: 40°C and below are "cool",
    /// 90°C and above are "hot".
    pub fn temp_color(&self, celsius: f32) -> Color {
        gradient(&self.temp, (celsius - 40.0) / 50.0)
    }

    /// Gradient color for a usage percentage.
    pub fn usage_color(&self, percent: f32) -> Color {
        gradient(&self.usage, percent / 100.0)
    }
}

/// Interpolate through a three-stop RGB gradient, clamped to [0, 1].
/// Non-RGB stops fall back to stepped selection.
pub fn gradient(stops: &[Color; 3], t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (a, b, local) = if t < 0.5 {
        (stops[0], stops[1], t * 2.0)
    } else {
        (stops[1], stops[2], (t - 0.5) * 2.0)
    };
    match (to_rgb(a), to_rgb(b)) {
        (Some((r1, g1, b1)), Some((r2, g2, b2))) => Color::Rgb(
            lerp(r1, r2, local),
            lerp(g1, g2, local),
            lerp(b1, b2, local),
        ),
        _ => {
            if local < 0.5 {
                a
            } else {
                b
            }
        }
    }
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}

fn to_rgb(color: Color) -> Option<(u8, u8, u8)> {
    match color {
        Color::Rgb(r, g, b) => Some((r, g, b)),
        _ => None,
    }
}

/// Errors from [`Theme::from_toml`].
#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("invalid theme TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("unknown base theme {name:?} (expected one of {BUILTIN_THEMES:?})")]
    UnknownBase { name: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeToml {
    name: Option<String>,
    base: Option<String>,
    bg: Option<ColorDef>,
    fg: Option<ColorDef>,
    muted: Option<ColorDef>,
    accent: Option<ColorDef>,
    border: Option<ColorDef>,
    title: Option<ColorDef>,
    ok: Option<ColorDef>,
    warn: Option<ColorDef>,
    crit: Option<ColorDef>,
    cpu: Option<ColorDef>,
    memory: Option<ColorDef>,
    disk: Option<ColorDef>,
    net_rx: Option<ColorDef>,
    net_tx: Option<ColorDef>,
    temp_low: Option<ColorDef>,
    temp_mid: Option<ColorDef>,
    temp_high: Option<ColorDef>,
    usage_low: Option<ColorDef>,
    usage_mid: Option<ColorDef>,
    usage_high: Option<ColorDef>,
    gauge_bg: Option<ColorDef>,
    selection_bg: Option<ColorDef>,
    selection_fg: Option<ColorDef>,
}

/// A color written as `"#rrggbb"` / `"#rgb"` or a ratatui color name like
/// `"green"` or `"lightblue"`.
#[derive(Debug)]
struct ColorDef(Color);

impl<'de> Deserialize<'de> for ColorDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        parse_color(&value)
            .map(ColorDef)
            .map_err(serde::de::Error::custom)
    }
}

fn parse_color(value: &str) -> Result<Color, String> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        let (r, g, b) = match hex.len() {
            3 => {
                let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16);
                let mut chars = hex.chars();
                let (r, g, b) = (chars.next(), chars.next(), chars.next());
                match (r, g, b) {
                    (Some(r), Some(g), Some(b)) => {
                        (expand(r), expand(g), expand(b))
                    }
                    _ => return Err(format!("invalid hex color {value:?}")),
                }
            }
            6 => (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ),
            _ => return Err(format!("invalid hex color {value:?}")),
        };
        match (r, g, b) {
            (Ok(r), Ok(g), Ok(b)) => Ok(Color::Rgb(r, g, b)),
            _ => Err(format!("invalid hex color {value:?}")),
        }
    } else {
        Color::from_str(value).map_err(|_| format!("unknown color {value:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_lookup() {
        assert_eq!(Theme::builtin("dark").unwrap(), Theme::dark());
        assert_eq!(Theme::builtin("LIGHT").unwrap(), Theme::light());
        assert!(Theme::builtin("neon").is_none());
    }

    #[test]
    fn toml_partial_override_falls_back_to_dark() {
        let theme = Theme::from_toml(
            r##"
            accent = "#ff8800"
            net_rx = "green"
            temp_high = "#0000ff"
            "##,
        )
        .unwrap();
        assert_eq!(theme.accent, Color::Rgb(0xff, 0x88, 0x00));
        assert_eq!(theme.net_rx, Color::from_str("green").unwrap());
        assert_eq!(theme.temp[2], Color::Rgb(0x00, 0x00, 0xff));
        assert_eq!(theme.cpu, Theme::dark().cpu);
    }

    #[test]
    fn toml_base_and_name() {
        let theme = Theme::from_toml(r##"base = "light"
name = "daylight"
cursor = 1"##);
        assert!(theme.is_err()); // unknown field rejected

        let theme = Theme::from_toml(r##"base = "light"
name = "daylight""##)
            .unwrap();
        assert_eq!(theme.name, "daylight");
        assert_eq!(theme.bg, Theme::light().bg);
    }

    #[test]
    fn toml_rejects_bad_base_and_colors() {
        assert!(matches!(
            Theme::from_toml(r#"base = "neon""#),
            Err(ThemeError::UnknownBase { .. })
        ));
        assert!(Theme::from_toml(r##"accent = "#12345""##).is_err());
        assert!(Theme::from_toml(r#"accent = "notacolor""#).is_err());
    }

    #[test]
    fn gradients_interpolate_and_clamp() {
        let theme = Theme::dark();
        assert_eq!(theme.usage_color(-10.0), theme.usage[0]);
        assert_eq!(theme.usage_color(200.0), theme.usage[2]);
        assert_eq!(theme.temp_color(15.0), theme.temp[0]);
        assert_eq!(theme.temp_color(120.0), theme.temp[2]);
        // midpoint lands on the middle stop
        assert_eq!(theme.usage_color(50.0), theme.usage[1]);
    }
}
