//! The shortcut that opens the grid: which keys can be chosen, how a choice is named, what
//! Windows is known to use it for, and how its availability is worded for the user.
//!
//! This module has no Win32 dependency, so it can be tested on any platform:
//! `rustc --test src/combo.rs -o /tmp/combo_test && /tmp/combo_test`

// The values of the Win32 MOD_* flags and VK_* codes, spelled out so that nothing here needs the
// `windows` crate.
pub const ALT: u32 = 0x1;
pub const CONTROL: u32 = 0x2;
pub const SHIFT: u32 = 0x4;
pub const WIN: u32 = 0x8;
const ALL_MODIFIERS: u32 = ALT | CONTROL | SHIFT | WIN;

const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_INSERT: u32 = 0x2D;
const VK_DELETE: u32 = 0x2E;
const VK_F1: u32 = 0x70;
const VK_F4: u32 = 0x73;

/// Keys offered after A-Z, 0-9 and F1-F12. Only keys whose name does not depend on the keyboard
/// layout are offered, so a label always matches the physical key.
const NAMED_KEYS: [(&str, u32); 7] = [
    ("Space", VK_SPACE),
    ("Page Up", VK_PRIOR),
    ("Page Down", VK_NEXT),
    ("End", VK_END),
    ("Home", VK_HOME),
    ("Insert", VK_INSERT),
    ("Delete", VK_DELETE),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Combo {
    /// A combination of `ALT`, `CONTROL`, `SHIFT` and `WIN`.
    pub modifiers: u32,
    /// A virtual-key code.
    pub key: u32,
}

impl Combo {
    /// The shortcut Tessera uses until the user picks another one.
    pub const DEFAULT: Combo = Combo { modifiers: WIN | SHIFT, key: b'G' as u32 };

    /// Tried when neither the saved nor the default shortcut can be registered.
    pub const FALLBACK: Combo = Combo { modifiers: WIN | CONTROL | ALT, key: b'G' as u32 };

    /// A usable shortcut has at least one modifier and a key from the list.
    pub fn is_valid(self) -> bool {
        self.modifiers != 0
            && self.modifiers & !ALL_MODIFIERS == 0
            && key_name(self.key).is_some()
    }

    /// For example "Win+Shift+G".
    pub fn label(self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (flag, name) in [(WIN, "Win"), (CONTROL, "Ctrl"), (ALT, "Alt"), (SHIFT, "Shift")] {
            if self.modifiers & flag != 0 {
                parts.push(name.to_string());
            }
        }
        parts.push(key_name(self.key).unwrap_or_else(|| format!("0x{:02X}", self.key)));
        parts.join("+")
    }
}

/// Every key that can be chosen, in the order shown in the panel: A-Z, 0-9, F1-F12, then the
/// named keys.
pub fn key_list() -> Vec<(String, u32)> {
    let mut keys = Vec::new();
    for vk in b'A'..=b'Z' {
        keys.push(((vk as char).to_string(), vk as u32));
    }
    for vk in b'0'..=b'9' {
        keys.push(((vk as char).to_string(), vk as u32));
    }
    for n in 0..12u32 {
        keys.push((format!("F{}", n + 1), VK_F1 + n));
    }
    for (name, vk) in NAMED_KEYS {
        keys.push((name.to_string(), vk));
    }
    keys
}

pub fn key_name(vk: u32) -> Option<String> {
    key_list().into_iter().find(|(_, code)| *code == vk).map(|(name, _)| name)
}

/// What Windows does with a combination, taken from Microsoft's keyboard shortcut
/// documentation, limited to the combinations the panel can produce. This is a hint for the
/// user; whether a combination is really free is decided by asking Windows to register it.
pub fn windows_use(combo: Combo) -> Option<&'static str> {
    const KNOWN: &[(u32, u32, &str)] = &[
        (WIN, b'A' as u32, "Quick Settings"),
        (WIN, b'B' as u32, "focus the notification area"),
        (WIN, b'C' as u32, "Copilot"),
        (WIN, b'D' as u32, "show or hide the desktop"),
        (WIN, b'E' as u32, "File Explorer"),
        (WIN, b'F' as u32, "Feedback Hub"),
        (WIN, b'G' as u32, "Xbox Game Bar"),
        (WIN, b'H' as u32, "voice typing"),
        (WIN, b'I' as u32, "Settings"),
        (WIN, b'K' as u32, "Cast"),
        (WIN, b'L' as u32, "lock the PC"),
        (WIN, b'M' as u32, "minimize all windows"),
        (WIN, b'N' as u32, "notifications and calendar"),
        (WIN, b'O' as u32, "lock the device orientation"),
        (WIN, b'P' as u32, "project to a display"),
        (WIN, b'Q' as u32, "Search"),
        (WIN, b'R' as u32, "Run"),
        (WIN, b'S' as u32, "Search"),
        (WIN, b'T' as u32, "cycle through taskbar apps"),
        (WIN, b'U' as u32, "accessibility settings"),
        (WIN, b'V' as u32, "clipboard history"),
        (WIN, b'W' as u32, "Widgets"),
        (WIN, b'X' as u32, "the quick link menu"),
        (WIN, b'Y' as u32, "switching input between Windows Mixed Reality and the desktop"),
        (WIN, b'Z' as u32, "snap layouts"),
        (WIN, VK_SPACE, "switching the input language"),
        (WIN, VK_HOME, "minimizing all windows except the active one"),
        (WIN | SHIFT, b'S' as u32, "Snipping Tool"),
        (WIN | SHIFT, b'R' as u32, "screen recording in Snipping Tool"),
        (WIN | SHIFT, b'M' as u32, "restoring minimized windows"),
        (WIN | SHIFT, b'T' as u32, "cycling through taskbar apps backwards"),
        (WIN | SHIFT, b'V' as u32, "cycling through notifications backwards"),
        (WIN | CONTROL, b'C' as u32, "color filters"),
        (WIN | CONTROL, b'D' as u32, "a new virtual desktop"),
        (WIN | CONTROL, b'F' as u32, "finding PCs"),
        (WIN | CONTROL, b'N' as u32, "Narrator settings"),
        (WIN | CONTROL, b'O' as u32, "the on-screen keyboard"),
        (WIN | CONTROL, b'Q' as u32, "Quick Assist"),
        (WIN | CONTROL | SHIFT, b'B' as u32, "waking the screen and resetting the graphics driver"),
        (WIN | ALT, b'B' as u32, "switching HDR on or off"),
        (WIN | ALT, b'D' as u32, "the date and time on the desktop"),
        (WIN | ALT, b'G' as u32, "Xbox Game Bar: record the last 30 seconds"),
        (WIN | ALT, b'R' as u32, "Xbox Game Bar: start or stop recording"),
        (CONTROL | ALT, VK_DELETE, "the Windows security screen"),
        (ALT, VK_F4, "closing the active window"),
        (ALT, VK_SPACE, "the window menu"),
    ];
    if let Some((_, _, name)) = KNOWN.iter().find(|(m, k, _)| *m == combo.modifiers && *k == combo.key) {
        return Some(name);
    }
    // Win+1 ... Win+9 and Win+0 start or switch to the taskbar apps.
    if combo.modifiers == WIN && (b'0' as u32..=b'9' as u32).contains(&combo.key) {
        return Some("starting or switching to a taskbar app");
    }
    None
}

/// What asking Windows to register a combination revealed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Availability {
    /// Tessera itself holds this combination right now.
    Active,
    /// Windows accepted the registration.
    Free,
    /// Windows refused it: the shell or another program already owns the combination.
    Taken,
}

/// How a status line should be colored.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Neutral,
    Good,
    Warning,
    Bad,
}

/// The sentence shown under the key picker for the chosen combination.
pub fn status(combo: Combo, availability: Availability) -> (Tone, String) {
    if combo.modifiers == 0 {
        return (Tone::Bad, "Choose at least one of Win, Ctrl, Alt or Shift.".to_string());
    }
    let label = combo.label();
    match (availability, windows_use(combo)) {
        (Availability::Active, _) => (
            Tone::Neutral,
            format!("{label} is the shortcut Tessera is using now."),
        ),
        (Availability::Taken, Some(what)) => (
            Tone::Bad,
            format!("{label} is already used by Windows for {what}. Pick another combination."),
        ),
        (Availability::Taken, None) => (
            Tone::Bad,
            format!("{label} is already taken by Windows or another program. Pick another combination."),
        ),
        (Availability::Free, Some(what)) => (
            Tone::Warning,
            format!("{label} is free right now, but Windows uses it for {what}. It may stop working."),
        ),
        (Availability::Free, None) => (Tone::Good, format!("{label} is available.")),
    }
}

/// Short note appended to a key in the drop-down list; empty when there is nothing to say.
pub fn list_note(combo: Combo, availability: Availability) -> &'static str {
    match (availability, windows_use(combo)) {
        (Availability::Active, _) => "  (current)",
        (Availability::Taken, Some(_)) => "  (used by Windows)",
        (Availability::Taken, None) => "  (taken)",
        (Availability::Free, Some(_)) => "  (Windows shortcut)",
        (Availability::Free, None) => "",
    }
}

/// A combination can be saved when it is valid and not owned by someone else.
pub fn can_save(combo: Combo, availability: Availability) -> bool {
    combo.is_valid() && availability != Availability::Taken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_windows_order() {
        assert_eq!(Combo::DEFAULT.label(), "Win+Shift+G");
        assert_eq!(Combo::FALLBACK.label(), "Win+Ctrl+Alt+G");
        let all = Combo { modifiers: ALT | CONTROL | SHIFT | WIN, key: VK_F1 + 11 };
        assert_eq!(all.label(), "Win+Ctrl+Alt+Shift+F12");
        let named = Combo { modifiers: CONTROL, key: VK_NEXT };
        assert_eq!(named.label(), "Ctrl+Page Down");
    }

    #[test]
    fn validity() {
        assert!(Combo::DEFAULT.is_valid());
        assert!(Combo::FALLBACK.is_valid());
        assert!(!Combo { modifiers: 0, key: b'G' as u32 }.is_valid());
        assert!(!Combo { modifiers: 0x10, key: b'G' as u32 }.is_valid());
        assert!(!Combo { modifiers: WIN, key: 0xBA }.is_valid());
        assert!(!Combo { modifiers: WIN, key: 0 }.is_valid());
    }

    #[test]
    fn key_list_is_complete_and_unique() {
        let keys = key_list();
        assert_eq!(keys.len(), 26 + 10 + 12 + NAMED_KEYS.len());
        let mut codes: Vec<u32> = keys.iter().map(|(_, c)| *c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), keys.len());
        let mut names: Vec<&String> = keys.iter().map(|(n, _)| n).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), keys.len());
        assert_eq!(key_name(b'Q' as u32).as_deref(), Some("Q"));
        assert_eq!(key_name(VK_F1 + 2).as_deref(), Some("F3"));
        assert_eq!(key_name(0xBA), None);
    }

    #[test]
    fn known_windows_shortcuts() {
        let snip = Combo { modifiers: WIN | SHIFT, key: b'S' as u32 };
        assert_eq!(windows_use(snip), Some("Snipping Tool"));
        assert_eq!(windows_use(Combo::DEFAULT), None);
        assert_eq!(windows_use(Combo::FALLBACK), None);
        let win_g = Combo { modifiers: WIN, key: b'G' as u32 };
        assert_eq!(windows_use(win_g), Some("Xbox Game Bar"));
        let win_5 = Combo { modifiers: WIN, key: b'5' as u32 };
        assert_eq!(windows_use(win_5), Some("starting or switching to a taskbar app"));
        let alt_f4 = Combo { modifiers: ALT, key: VK_F4 };
        assert!(windows_use(alt_f4).is_some());
    }

    #[test]
    fn every_known_combination_can_be_produced_by_the_panel() {
        // Whatever the table lists must be selectable, otherwise the entry is dead weight.
        let keys: Vec<u32> = key_list().into_iter().map(|(_, c)| c).collect();
        for modifiers in 1..=ALL_MODIFIERS {
            for key in &keys {
                let combo = Combo { modifiers, key: *key };
                if windows_use(combo).is_some() {
                    assert!(combo.is_valid(), "{} is listed but invalid", combo.label());
                }
            }
        }
    }

    #[test]
    fn status_wording() {
        let snip = Combo { modifiers: WIN | SHIFT, key: b'S' as u32 };
        let (tone, text) = status(snip, Availability::Taken);
        assert_eq!(tone, Tone::Bad);
        assert_eq!(
            text,
            "Win+Shift+S is already used by Windows for Snipping Tool. Pick another combination."
        );

        let (tone, text) = status(Combo::DEFAULT, Availability::Taken);
        assert_eq!(tone, Tone::Bad);
        assert!(text.contains("another program"));

        let (tone, text) = status(Combo::DEFAULT, Availability::Free);
        assert_eq!(tone, Tone::Good);
        assert_eq!(text, "Win+Shift+G is available.");

        let (tone, _) = status(Combo::DEFAULT, Availability::Active);
        assert_eq!(tone, Tone::Neutral);

        let win_l = Combo { modifiers: WIN, key: b'L' as u32 };
        let (tone, text) = status(win_l, Availability::Free);
        assert_eq!(tone, Tone::Warning);
        assert!(text.contains("lock the PC"));

        let none = Combo { modifiers: 0, key: b'G' as u32 };
        assert_eq!(status(none, Availability::Free).0, Tone::Bad);
    }

    #[test]
    fn list_notes_and_saving() {
        let snip = Combo { modifiers: WIN | SHIFT, key: b'S' as u32 };
        assert_eq!(list_note(snip, Availability::Taken), "  (used by Windows)");
        assert_eq!(list_note(Combo::DEFAULT, Availability::Taken), "  (taken)");
        assert_eq!(list_note(Combo::DEFAULT, Availability::Free), "");
        assert_eq!(list_note(Combo::DEFAULT, Availability::Active), "  (current)");

        assert!(can_save(Combo::DEFAULT, Availability::Free));
        assert!(can_save(Combo::DEFAULT, Availability::Active));
        assert!(!can_save(Combo::DEFAULT, Availability::Taken));
        assert!(!can_save(Combo { modifiers: 0, key: b'G' as u32 }, Availability::Free));
        // A free combination that Windows documents is allowed, with a warning.
        assert!(can_save(Combo { modifiers: WIN, key: b'L' as u32 }, Availability::Free));
    }
}
