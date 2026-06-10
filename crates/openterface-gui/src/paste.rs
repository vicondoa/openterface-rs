//! Focused-GUI paste configuration and hotkey helpers.

use std::collections::BTreeSet;

use openterface_core::event::{HidUsage, Modifiers};

/// Environment variable that enables/disables focused GUI paste.
pub(crate) const ENV_ENABLE_PASTE: &str = "OPENTERFACE_ENABLE_PASTE";
/// Environment variable that caps normalized characters per paste.
pub(crate) const ENV_PASTE_MAX_CHARS: &str = "OPENTERFACE_PASTE_MAX_CHARS";
/// Environment variable that sets the focused GUI paste shortcut.
pub(crate) const ENV_PASTE_SHORTCUT: &str = "OPENTERFACE_PASTE_SHORTCUT";
/// Environment variable that controls host-side paste on middle mouse click.
pub(crate) const ENV_MIDDLE_CLICK_PASTE: &str = "OPENTERFACE_MIDDLE_CLICK_PASTE";

/// Default normalized-character paste cap.
pub(crate) const DEFAULT_PASTE_MAX_CHARS: usize = 4096;
/// Maximum accepted normalized-character paste cap.
const MAX_PASTE_MAX_CHARS: usize = 65_536;

const HID_V: HidUsage = HidUsage(0x19);

/// A focused-GUI host-local paste shortcut.
///
/// The first implementation supports modifier combinations over physical `V`.
/// Modifier families are matched exactly (left/right sides are accepted), so
/// `Ctrl+Shift+V` does not also steal `Ctrl+Alt+Shift+V`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct PasteShortcut {
    ctrl: bool,
    alt: bool,
    shift: bool,
    super_key: bool,
    key: HidUsage,
}

impl PasteShortcut {
    const DEFAULT: Self = Self {
        ctrl: true,
        alt: false,
        shift: true,
        super_key: false,
        key: HID_V,
    };

    #[must_use]
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut shortcut = Self {
            ctrl: false,
            alt: false,
            shift: false,
            super_key: false,
            key: HidUsage(0),
        };
        let mut saw_key = false;
        for raw in value
            .split(['+', '-', ' ', '_'])
            .filter(|token| !token.is_empty())
        {
            match raw.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => shortcut.ctrl = true,
                "alt" => shortcut.alt = true,
                "shift" => shortcut.shift = true,
                "super" | "meta" | "mod" | "win" | "logo" => shortcut.super_key = true,
                "v" => {
                    shortcut.key = HID_V;
                    saw_key = true;
                }
                _ => return None,
            }
        }
        let has_modifier = shortcut.ctrl || shortcut.alt || shortcut.shift || shortcut.super_key;
        if saw_key && has_modifier {
            Some(shortcut)
        } else {
            None
        }
    }

    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match (self.ctrl, self.alt, self.shift, self.super_key, self.key) {
            (true, false, true, false, HID_V) => "Ctrl+Shift+V",
            (true, true, true, false, HID_V) => "Ctrl+Alt+Shift+V",
            (true, false, false, false, HID_V) => "Ctrl+V",
            (false, true, false, false, HID_V) => "Alt+V",
            (false, false, true, false, HID_V) => "Shift+V",
            (false, false, false, true, HID_V) => "Super+V",
            (true, false, false, true, HID_V) => "Ctrl+Super+V",
            (true, false, true, true, HID_V) => "Ctrl+Shift+Super+V",
            (true, true, false, false, HID_V) => "Ctrl+Alt+V",
            (false, true, true, false, HID_V) => "Alt+Shift+V",
            _ => "custom modifier+V",
        }
    }
}

/// Runtime paste settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct PasteConfig {
    pub enabled: bool,
    pub max_chars: usize,
    pub shortcut: PasteShortcut,
    pub middle_click: MiddleClickPaste,
}

/// Host-side paste behavior for middle mouse clicks.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) enum MiddleClickPaste {
    /// Forward middle click to the target unchanged.
    #[default]
    Off,
    /// Paste the host primary selection.
    Primary,
    /// Paste the host regular clipboard.
    Clipboard,
}

impl MiddleClickPaste {
    #[must_use]
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "0" | "false" | "no" | "off" | "forward" | "target" => Some(Self::Off),
            "1" | "true" | "yes" | "on" | "primary" | "selection" => Some(Self::Primary),
            "clipboard" | "regular" => Some(Self::Clipboard),
            _ => None,
        }
    }
}

/// Which host selection to paste.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PasteSource {
    /// Regular clipboard (`Ctrl+C` / `Ctrl+V`).
    Clipboard,
    /// Primary selection (mouse selection / middle-click paste).
    Primary,
}

impl PasteSource {
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            PasteSource::Clipboard => "clipboard",
            PasteSource::Primary => "primary selection",
        }
    }
}

impl PasteConfig {
    /// Builds paste config from `OPENTERFACE_*` environment variables.
    #[must_use]
    pub(crate) fn from_env() -> Self {
        let enabled = std::env::var(ENV_ENABLE_PASTE)
            .ok()
            .map(|v| !matches!(v.trim(), "0" | "false" | "no" | "off"))
            .unwrap_or(true);
        let max_chars = std::env::var(ENV_PASTE_MAX_CHARS)
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|&n| (1..=MAX_PASTE_MAX_CHARS).contains(&n))
            .unwrap_or(DEFAULT_PASTE_MAX_CHARS);
        let shortcut = std::env::var(ENV_PASTE_SHORTCUT)
            .ok()
            .and_then(|v| PasteShortcut::parse(&v))
            .unwrap_or(PasteShortcut::DEFAULT);
        let middle_click = std::env::var(ENV_MIDDLE_CLICK_PASTE)
            .ok()
            .and_then(|v| MiddleClickPaste::parse(&v))
            .unwrap_or_default();
        Self {
            enabled,
            max_chars,
            shortcut,
            middle_click,
        }
    }
}

/// Returns true for the configured focused-GUI paste hotkey.
#[must_use]
pub(crate) fn is_paste_hotkey(
    usage: HidUsage,
    modifiers: Modifiers,
    shortcut: PasteShortcut,
) -> bool {
    usage == shortcut.key
        && has_any_ctrl(modifiers) == shortcut.ctrl
        && has_any_alt(modifiers) == shortcut.alt
        && has_any_shift(modifiers) == shortcut.shift
        && has_any_super(modifiers) == shortcut.super_key
}

/// Returns the HID usages that belong to the active paste chord.
#[must_use]
pub(crate) fn chord_usages(trigger: HidUsage, modifiers: Modifiers) -> Vec<HidUsage> {
    let mut out = vec![trigger];
    for (bit, usage) in [
        (Modifiers::LEFT_CTRL, HidUsage(0xE0)),
        (Modifiers::LEFT_SHIFT, HidUsage(0xE1)),
        (Modifiers::LEFT_ALT, HidUsage(0xE2)),
        (Modifiers::LEFT_GUI, HidUsage(0xE3)),
        (Modifiers::RIGHT_CTRL, HidUsage(0xE4)),
        (Modifiers::RIGHT_SHIFT, HidUsage(0xE5)),
        (Modifiers::RIGHT_ALT, HidUsage(0xE6)),
        (Modifiers::RIGHT_GUI, HidUsage(0xE7)),
    ] {
        if modifiers.contains(bit) {
            out.push(usage);
        }
    }
    out
}

/// Tracks keys whose press/repeat/release edges must be swallowed until physical
/// release because they formed a host-local shortcut.
#[derive(Default, Debug)]
pub(crate) struct SuppressedKeys {
    keys: BTreeSet<HidUsage>,
}

impl SuppressedKeys {
    pub(crate) fn extend<I>(&mut self, usages: I)
    where
        I: IntoIterator<Item = HidUsage>,
    {
        self.keys.extend(usages);
    }

    /// Returns true if this event should be swallowed. Press/repeat events stay
    /// suppressed; a release removes the key from the suppression set.
    pub(crate) fn suppress_event(&mut self, usage: HidUsage, pressed: bool) -> bool {
        if !self.keys.contains(&usage) {
            return false;
        }
        if !pressed {
            self.keys.remove(&usage);
        }
        true
    }
}

fn has_any_ctrl(modifiers: Modifiers) -> bool {
    modifiers.contains(Modifiers::LEFT_CTRL) || modifiers.contains(Modifiers::RIGHT_CTRL)
}

fn has_any_alt(modifiers: Modifiers) -> bool {
    modifiers.contains(Modifiers::LEFT_ALT) || modifiers.contains(Modifiers::RIGHT_ALT)
}

fn has_any_shift(modifiers: Modifiers) -> bool {
    modifiers.contains(Modifiers::LEFT_SHIFT) || modifiers.contains(Modifiers::RIGHT_SHIFT)
}

fn has_any_super(modifiers: Modifiers) -> bool {
    modifiers.contains(Modifiers::LEFT_GUI) || modifiers.contains(Modifiers::RIGHT_GUI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_hotkey_accepts_left_and_right_ctrl_shift() {
        assert!(is_paste_hotkey(
            HID_V,
            Modifiers::LEFT_CTRL.union(Modifiers::LEFT_SHIFT),
            PasteShortcut::DEFAULT
        ));
        assert!(is_paste_hotkey(
            HID_V,
            Modifiers::RIGHT_CTRL.union(Modifiers::RIGHT_SHIFT),
            PasteShortcut::DEFAULT
        ));
        assert!(!is_paste_hotkey(
            HID_V,
            Modifiers::LEFT_CTRL
                .union(Modifiers::LEFT_ALT)
                .union(Modifiers::LEFT_SHIFT),
            PasteShortcut::DEFAULT
        ));
        assert!(!is_paste_hotkey(
            HidUsage(0x04),
            Modifiers::LEFT_CTRL.union(Modifiers::LEFT_SHIFT),
            PasteShortcut::DEFAULT
        ));
    }

    #[test]
    fn configurable_shortcut_parses_common_modifier_v_combos() {
        let old = PasteShortcut::parse("ctrl-alt-shift-v").unwrap();
        assert_eq!(old.label(), "Ctrl+Alt+Shift+V");
        assert!(is_paste_hotkey(
            HID_V,
            Modifiers::LEFT_CTRL
                .union(Modifiers::LEFT_ALT)
                .union(Modifiers::LEFT_SHIFT),
            old
        ));
        assert!(!is_paste_hotkey(
            HID_V,
            Modifiers::LEFT_CTRL.union(Modifiers::LEFT_SHIFT),
            old
        ));
        assert_eq!(
            PasteShortcut::parse("control+shift+v"),
            Some(PasteShortcut::DEFAULT)
        );
        assert_eq!(PasteShortcut::parse("v"), None);
        assert_eq!(PasteShortcut::parse("ctrl-shift-x"), None);
    }

    #[test]
    fn paste_source_labels_are_stable() {
        assert_eq!(PasteSource::Clipboard.label(), "clipboard");
        assert_eq!(PasteSource::Primary.label(), "primary selection");
    }

    #[test]
    fn chord_usages_include_trigger_and_held_modifier_sides() {
        let usages = chord_usages(
            HID_V,
            Modifiers::LEFT_CTRL
                .union(Modifiers::RIGHT_ALT)
                .union(Modifiers::LEFT_SHIFT),
        );
        assert_eq!(
            usages,
            vec![HID_V, HidUsage(0xE0), HidUsage(0xE1), HidUsage(0xE6)]
        );
    }

    #[test]
    fn suppression_swallows_press_repeat_and_release_until_up_edge() {
        let mut suppressed = SuppressedKeys::default();
        suppressed.extend([HID_V, HidUsage(0xE0)]);
        assert!(suppressed.suppress_event(HID_V, true)); // initial/repeat press
        assert!(suppressed.suppress_event(HID_V, true)); // repeat
        assert!(suppressed.suppress_event(HID_V, false)); // release clears
        assert!(!suppressed.suppress_event(HID_V, false));
        assert!(suppressed.suppress_event(HidUsage(0xE0), true));
        assert!(suppressed.suppress_event(HidUsage(0xE0), false));
    }

    #[test]
    fn env_config_defaults_and_ranges() {
        std::env::remove_var(ENV_ENABLE_PASTE);
        std::env::remove_var(ENV_PASTE_MAX_CHARS);
        assert_eq!(
            PasteConfig::from_env(),
            PasteConfig {
                enabled: true,
                max_chars: DEFAULT_PASTE_MAX_CHARS,
                shortcut: PasteShortcut::DEFAULT,
                middle_click: MiddleClickPaste::Off,
            }
        );
        std::env::set_var(ENV_ENABLE_PASTE, "0");
        std::env::set_var(ENV_PASTE_MAX_CHARS, "12");
        std::env::set_var(ENV_PASTE_SHORTCUT, "ctrl-alt-shift-v");
        std::env::set_var(ENV_MIDDLE_CLICK_PASTE, "clipboard");
        assert_eq!(
            PasteConfig::from_env(),
            PasteConfig {
                enabled: false,
                max_chars: 12,
                shortcut: PasteShortcut::parse("ctrl-alt-shift-v").unwrap(),
                middle_click: MiddleClickPaste::Clipboard,
            }
        );
        std::env::set_var(ENV_MIDDLE_CLICK_PASTE, "off");
        assert_eq!(PasteConfig::from_env().middle_click, MiddleClickPaste::Off);
        std::env::set_var(ENV_MIDDLE_CLICK_PASTE, "invalid");
        assert_eq!(PasteConfig::from_env().middle_click, MiddleClickPaste::Off);
        std::env::set_var(ENV_PASTE_SHORTCUT, "not-a-shortcut");
        assert_eq!(PasteConfig::from_env().shortcut, PasteShortcut::DEFAULT);
        std::env::set_var(ENV_PASTE_MAX_CHARS, "0");
        assert_eq!(PasteConfig::from_env().max_chars, DEFAULT_PASTE_MAX_CHARS);
        std::env::remove_var(ENV_ENABLE_PASTE);
        std::env::remove_var(ENV_PASTE_MAX_CHARS);
        std::env::remove_var(ENV_PASTE_SHORTCUT);
        std::env::remove_var(ENV_MIDDLE_CLICK_PASTE);
    }
}
