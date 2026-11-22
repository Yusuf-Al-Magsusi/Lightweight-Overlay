//! Colour themes. A theme is three colours (text, background, accent); the
//! user can override any of them with a hex string in the config.

use serde::{Deserialize, Serialize};

use crate::widget::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Midnight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub text: Color,
    pub background: Color,
    pub accent: Color,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::Midnight];

    pub fn label(self) -> &'static str {
        match self {
            Theme::Dark => "Dark",
            Theme::Light => "Light",
            Theme::Midnight => "Midnight",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Theme::Dark => Palette {
                text: Color::new(1.0, 1.0, 1.0, 1.0),
                background: Color::new(0.05, 0.05, 0.08, 0.70),
                accent: Color::new(0.30, 0.80, 0.50, 1.0),
            },
            Theme::Light => Palette {
                text: Color::new(0.08, 0.08, 0.10, 1.0),
                background: Color::new(0.97, 0.97, 0.98, 0.85),
                accent: Color::new(0.10, 0.45, 0.85, 1.0),
            },
            Theme::Midnight => Palette {
                text: Color::new(0.85, 0.90, 1.0, 1.0),
                background: Color::new(0.02, 0.03, 0.10, 0.80),
                accent: Color::new(0.55, 0.45, 1.0, 1.0),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_readable_contrast_between_text_and_background() {
        for t in Theme::ALL {
            let p = t.palette();
            let lum = |c: Color| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
            assert!(
                (lum(p.text) - lum(p.background)).abs() > 0.5,
                "{} has low contrast",
                t.label()
            );
        }
    }

    #[test]
    fn default_theme_is_dark() {
        assert_eq!(Theme::default(), Theme::Dark);
    }

    #[test]
    fn labels_are_unique() {
        let labels: Vec<_> = Theme::ALL.iter().map(|t| t.label()).collect();
        for (i, a) in labels.iter().enumerate() {
            assert!(!labels[i + 1..].contains(a));
        }
    }
}
