//! Modifier tracking and the language switch hotkey (port of the hotkey logic
//! of OpenKey's keyboard hook).

use std::fmt;
use std::ops::BitOr;

use crate::keys::*;

/// Set of held modifier keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifier.
    pub const NONE: Self = Self(0);
    /// Shift.
    pub const SHIFT: Self = Self(0x01);
    /// Control.
    pub const CONTROL: Self = Self(0x02);
    /// Alt / Option.
    pub const ALT: Self = Self(0x04);
    /// Windows / Command.
    pub const META: Self = Self(0x20);

    /// Raw bits.
    pub fn bits(self) -> u8 {
        self.0
    }

    /// Whether all modifiers of `other` are held.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    /// Whether no modifier is held.
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Adds modifiers.
    pub fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    /// Removes modifiers.
    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }

    /// The modifier produced by a key, if it is a modifier key.
    pub fn from_key(key: KeyCode) -> Option<Self> {
        match key {
            KEY_LEFT_SHIFT | KEY_RIGHT_SHIFT | KEY_SHIFT => Some(Self::SHIFT),
            KEY_LEFT_CONTROL | KEY_RIGHT_CONTROL | KEY_CONTROL => Some(Self::CONTROL),
            KEY_LEFT_ALT | KEY_RIGHT_ALT | KEY_ALT => Some(Self::ALT),
            KEY_LEFT_META | KEY_RIGHT_META => Some(Self::META),
            _ => None,
        }
    }
}

impl BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// A keyboard shortcut. A hotkey without `key` triggers when its modifiers
/// are pressed together and released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Hotkey {
    /// Requires Control.
    pub control: bool,
    /// Requires Alt / Option.
    pub alt: bool,
    /// Requires Shift.
    pub shift: bool,
    /// Requires Windows / Command.
    pub meta: bool,
    /// Optional non-modifier key.
    pub key: Option<KeyCode>,
    /// Beep when the hotkey switches language.
    pub beep: bool,
}

impl Default for Hotkey {
    /// `Ctrl + Shift`, like UniKey.
    fn default() -> Self {
        Self {
            control: true,
            alt: false,
            shift: true,
            meta: false,
            key: None,
            beep: false,
        }
    }
}

impl Hotkey {
    /// Whether the hotkey is disabled.
    pub fn is_empty(&self) -> bool {
        !self.control && !self.alt && !self.shift && !self.meta && self.key.is_none()
    }

    /// Required modifiers.
    pub fn modifiers(&self) -> Modifiers {
        let mut m = Modifiers::NONE;
        if self.shift {
            m.insert(Modifiers::SHIFT);
        }
        if self.control {
            m.insert(Modifiers::CONTROL);
        }
        if self.alt {
            m.insert(Modifiers::ALT);
        }
        if self.meta {
            m.insert(Modifiers::META);
        }
        m
    }

    fn matches(&self, held: Modifiers, key: Option<KeyCode>) -> bool {
        if self.is_empty() || self.modifiers() != held {
            return false;
        }
        match self.key {
            Some(k) => key == Some(k),
            None => true,
        }
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("(none)");
        }
        let mut parts: Vec<String> = Vec::new();
        if self.control {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        if self.meta {
            parts.push("Win".into());
        }
        if let Some(key) = self.key {
            let name = match key {
                KEY_SPACE => "Space".to_owned(),
                _ => key_code_to_char(u32::from(key))
                    .map(|c| c.to_ascii_uppercase().to_string())
                    .unwrap_or_else(|| format!("0x{key:02X}")),
            };
            parts.push(name);
        }
        f.write_str(&parts.join(" + "))
    }
}

/// Action triggered by modifier/hotkey handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Toggle Vietnamese / English.
    SwitchLanguage,
    /// Ctrl tapped alone: temporarily toggle spell checking.
    TempOffSpelling,
    /// Alt tapped alone: temporarily disable the engine for the current word.
    TempOffEngine,
}

/// What the host should do with the key after hotkey detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Disposition {
    /// The key was a hotkey and must be swallowed.
    Swallow,
    /// The key (a modifier) must pass through untouched.
    PassThrough,
    /// The key should be processed by the engine.
    Continue,
}

/// Tracks modifiers across key events and detects hotkeys.
#[derive(Debug, Clone, Default)]
pub struct HotkeyDetector {
    flag: Modifiers,
    last_flag: Modifiers,
    keycode: KeyCode,
    has_just_used_hotkey: bool,
}

impl HotkeyDetector {
    /// Currently held modifiers.
    pub fn modifiers(&self) -> Modifiers {
        self.flag
    }

    /// Clears the tracked modifiers (e.g. after the session was locked).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feeds a key event. Returns the disposition and triggered actions.
    pub fn handle(
        &mut self,
        key: KeyCode,
        down: bool,
        hotkey: &Hotkey,
        temp_off_spelling: bool,
        temp_off_engine: bool,
    ) -> (Disposition, Vec<HotkeyAction>) {
        let modifier = Modifiers::from_key(key);
        if let Some(m) = modifier {
            if down {
                self.flag.insert(m);
            } else {
                self.flag.remove(m);
            }
        } else if down {
            self.keycode = key;
        }

        let mut actions = Vec::new();
        if down && modifier.is_none() && self.keycode != 0 {
            if hotkey.key != Some(self.keycode) {
                self.last_flag = Modifiers::NONE;
            } else if hotkey.matches(self.last_flag, Some(self.keycode)) {
                actions.push(HotkeyAction::SwitchLanguage);
                self.has_just_used_hotkey = true;
                self.keycode = 0;
                return (Disposition::Swallow, actions);
            }
            self.has_just_used_hotkey = !self.last_flag.is_empty();
        } else if modifier.is_some() {
            if self.last_flag.is_empty() || self.last_flag < self.flag {
                self.last_flag = self.flag;
            } else if self.last_flag > self.flag {
                if hotkey.key.is_none() && hotkey.matches(self.last_flag, None) {
                    actions.push(HotkeyAction::SwitchLanguage);
                    self.has_just_used_hotkey = true;
                }
                if temp_off_spelling
                    && !self.has_just_used_hotkey
                    && self.last_flag.contains(Modifiers::CONTROL)
                {
                    actions.push(HotkeyAction::TempOffSpelling);
                }
                if temp_off_engine
                    && !self.has_just_used_hotkey
                    && self.last_flag.contains(Modifiers::ALT)
                {
                    actions.push(HotkeyAction::TempOffEngine);
                }
                self.last_flag = self.flag;
                // Keep the "used" state until every modifier is released so
                // releasing the rest of a chord does not trigger tap actions.
                self.has_just_used_hotkey = self.has_just_used_hotkey && !self.flag.is_empty();
            }
            self.keycode = 0;
            return (Disposition::PassThrough, actions);
        }
        (Disposition::Continue, actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_only_hotkey_fires_on_release() {
        let hotkey = Hotkey::default();
        let mut d = HotkeyDetector::default();
        assert!(d.handle(KEY_LEFT_CONTROL, true, &hotkey, false, false).1.is_empty());
        assert!(d.handle(KEY_LEFT_SHIFT, true, &hotkey, false, false).1.is_empty());
        let (_, actions) = d.handle(KEY_LEFT_SHIFT, false, &hotkey, false, false);
        assert_eq!(actions, vec![HotkeyAction::SwitchLanguage]);
        assert!(d.handle(KEY_LEFT_CONTROL, false, &hotkey, false, false).1.is_empty());
    }

    #[test]
    fn modifier_hotkey_is_cancelled_by_other_key() {
        let hotkey = Hotkey::default();
        let mut d = HotkeyDetector::default();
        d.handle(KEY_LEFT_CONTROL, true, &hotkey, false, false);
        d.handle(KEY_LEFT_SHIFT, true, &hotkey, false, false);
        d.handle(KEY_A, true, &hotkey, false, false);
        d.handle(KEY_A, false, &hotkey, false, false);
        assert!(d.handle(KEY_LEFT_SHIFT, false, &hotkey, false, false).1.is_empty());
        assert!(d.handle(KEY_LEFT_CONTROL, false, &hotkey, false, false).1.is_empty());
    }

    #[test]
    fn key_hotkey_is_swallowed() {
        let hotkey = Hotkey {
            control: false,
            alt: true,
            shift: false,
            meta: false,
            key: Some(KEY_Z),
            beep: false,
        };
        let mut d = HotkeyDetector::default();
        d.handle(KEY_LEFT_ALT, true, &hotkey, false, false);
        let (disposition, actions) = d.handle(KEY_Z, true, &hotkey, false, false);
        assert_eq!(disposition, Disposition::Swallow);
        assert_eq!(actions, vec![HotkeyAction::SwitchLanguage]);
        assert_eq!(hotkey.to_string(), "Alt + Z");
    }

    #[test]
    fn ctrl_tap_toggles_spelling() {
        let hotkey = Hotkey::default();
        let mut d = HotkeyDetector::default();
        d.handle(KEY_LEFT_CONTROL, true, &hotkey, true, false);
        let (_, actions) = d.handle(KEY_LEFT_CONTROL, false, &hotkey, true, false);
        assert_eq!(actions, vec![HotkeyAction::TempOffSpelling]);
    }
}
