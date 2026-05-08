// SPDX-License-Identifier: Apache-2.0
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
            Self::Default => include_str!("themes/default.css"),
            Self::Dark => include_str!("themes/dark.css"),
            Self::Solarized => include_str!("themes/solarized.css"),
        }
    }
}
