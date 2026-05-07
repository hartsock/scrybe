// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright 2026 Shawn Hartsock and contributors

//! Render themes — fresh CSS, not MacDown's stylesheet.

/// Available render themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Default,
    Dark,
    Solarized,
}

impl Theme {
    /// Returns the CSS string for this theme.
    pub fn css(&self) -> &'static str {
        match self {
            Theme::Default => include_str!("themes/default.css"),
            Theme::Dark => include_str!("themes/dark.css"),
            Theme::Solarized => include_str!("themes/solarized.css"),
        }
    }
}
