//! The grid sizes the options panel offers.
//!
//! This module has no Win32 dependency, so it can be tested on any platform:
//! `rustc --test src/grid.rs -o /tmp/grid_test && /tmp/grid_test`

/// Sizes offered in the options panel; every grid has as many columns as rows.
pub const PRESETS: [i32; 4] = [3, 4, 6, 8];

/// The size used until the user picks another one.
pub const DEFAULT: i32 = 6;

pub fn is_valid(size: i32) -> bool {
    PRESETS.contains(&size)
}

/// For example "6 × 6".
pub fn label(size: i32) -> String {
    format!("{size} × {size}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_include_the_default() {
        assert!(is_valid(DEFAULT));
        assert_eq!(PRESETS, [3, 4, 6, 8]);
    }

    #[test]
    fn only_presets_are_valid() {
        for size in [-1, 0, 1, 2, 5, 7, 9, 100] {
            assert!(!is_valid(size), "{size} should not be valid");
        }
        for size in PRESETS {
            assert!(is_valid(size));
        }
    }

    #[test]
    fn labels() {
        assert_eq!(label(3), "3 × 3");
        assert_eq!(label(8), "8 × 8");
    }
}
