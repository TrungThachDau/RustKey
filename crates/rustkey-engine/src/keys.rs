//! Platform independent key codes used by the engine.
//!
//! The values intentionally match Windows virtual-key codes (US layout) so the
//! Windows host can pass `vkCode` straight through. Other platforms only need to
//! translate their native key codes into these constants.

/// A key code understood by the engine.
pub type KeyCode = u16;

pub const KEY_ESC: KeyCode = 0x1B;
/// Backspace.
pub const KEY_DELETE: KeyCode = 0x08;
pub const KEY_TAB: KeyCode = 0x09;
pub const KEY_ENTER: KeyCode = 0x0D;
pub const KEY_RETURN: KeyCode = 0x0D;
pub const KEY_SPACE: KeyCode = 0x20;
pub const KEY_LEFT: KeyCode = 0x25;
pub const KEY_UP: KeyCode = 0x26;
pub const KEY_RIGHT: KeyCode = 0x27;
pub const KEY_DOWN: KeyCode = 0x28;

pub const KEY_PAGE_UP: KeyCode = 0x21;
pub const KEY_PAGE_DOWN: KeyCode = 0x22;
pub const KEY_END: KeyCode = 0x23;
pub const KEY_HOME: KeyCode = 0x24;
pub const KEY_SELECT: KeyCode = 0x29;
pub const KEY_PRINT: KeyCode = 0x2A;
pub const KEY_EXECUTE: KeyCode = 0x2B;
pub const KEY_PRINT_SCREEN: KeyCode = 0x2C;
pub const KEY_INSERT: KeyCode = 0x2D;
/// Forward delete.
pub const KEY_FORWARD_DELETE: KeyCode = 0x2E;
pub const KEY_HELP: KeyCode = 0x2F;
pub const KEY_NUM_LOCK: KeyCode = 0x90;
pub const KEY_SCROLL_LOCK: KeyCode = 0x91;

/// Left shift.
pub const KEY_LEFT_SHIFT: KeyCode = 0xA0;
/// Right shift.
pub const KEY_RIGHT_SHIFT: KeyCode = 0xA1;
/// Left control.
pub const KEY_LEFT_CONTROL: KeyCode = 0xA2;
/// Right control.
pub const KEY_RIGHT_CONTROL: KeyCode = 0xA3;
/// Left alt / option.
pub const KEY_LEFT_ALT: KeyCode = 0xA4;
/// Right alt / option.
pub const KEY_RIGHT_ALT: KeyCode = 0xA5;
/// Left Windows / command key.
pub const KEY_LEFT_META: KeyCode = 0x5B;
/// Right Windows / command key.
pub const KEY_RIGHT_META: KeyCode = 0x5C;
/// Generic shift (side unknown).
pub const KEY_SHIFT: KeyCode = 0x10;
/// Generic control (side unknown).
pub const KEY_CONTROL: KeyCode = 0x11;
/// Generic alt (side unknown).
pub const KEY_ALT: KeyCode = 0x12;

pub const KEY_A: KeyCode = 0x41;
pub const KEY_B: KeyCode = 0x42;
pub const KEY_C: KeyCode = 0x43;
pub const KEY_D: KeyCode = 0x44;
pub const KEY_E: KeyCode = 0x45;
pub const KEY_F: KeyCode = 0x46;
pub const KEY_G: KeyCode = 0x47;
pub const KEY_H: KeyCode = 0x48;
pub const KEY_I: KeyCode = 0x49;
pub const KEY_J: KeyCode = 0x4A;
pub const KEY_K: KeyCode = 0x4B;
pub const KEY_L: KeyCode = 0x4C;
pub const KEY_M: KeyCode = 0x4D;
pub const KEY_N: KeyCode = 0x4E;
pub const KEY_O: KeyCode = 0x4F;
pub const KEY_P: KeyCode = 0x50;
pub const KEY_Q: KeyCode = 0x51;
pub const KEY_R: KeyCode = 0x52;
pub const KEY_S: KeyCode = 0x53;
pub const KEY_T: KeyCode = 0x54;
pub const KEY_U: KeyCode = 0x55;
pub const KEY_V: KeyCode = 0x56;
pub const KEY_W: KeyCode = 0x57;
pub const KEY_X: KeyCode = 0x58;
pub const KEY_Y: KeyCode = 0x59;
pub const KEY_Z: KeyCode = 0x5A;

pub const KEY_0: KeyCode = 0x30;
pub const KEY_1: KeyCode = 0x31;
pub const KEY_2: KeyCode = 0x32;
pub const KEY_3: KeyCode = 0x33;
pub const KEY_4: KeyCode = 0x34;
pub const KEY_5: KeyCode = 0x35;
pub const KEY_6: KeyCode = 0x36;
pub const KEY_7: KeyCode = 0x37;
pub const KEY_8: KeyCode = 0x38;
pub const KEY_9: KeyCode = 0x39;

pub const KEY_SEMICOLON: KeyCode = 0xBA;
pub const KEY_EQUALS: KeyCode = 0xBB;
pub const KEY_COMMA: KeyCode = 0xBC;
pub const KEY_MINUS: KeyCode = 0xBD;
pub const KEY_DOT: KeyCode = 0xBE;
pub const KEY_SLASH: KeyCode = 0xBF;
pub const KEY_BACKQUOTE: KeyCode = 0xC0;
pub const KEY_LEFT_BRACKET: KeyCode = 0xDB;
pub const KEY_BACK_SLASH: KeyCode = 0xDC;
pub const KEY_RIGHT_BRACKET: KeyCode = 0xDD;
pub const KEY_QUOTE: KeyCode = 0xDE;

/// Returns `true` for the letter keys `A`..=`Z`.
pub fn is_letter(key: KeyCode) -> bool {
    (KEY_A..=KEY_Z).contains(&key)
}

/// Returns `true` for the digit keys of the main keyboard row.
pub fn is_number(key: KeyCode) -> bool {
    (KEY_0..=KEY_9).contains(&key)
}

/// `(character, key code, needs shift)` for every printable ASCII character.
const CHARACTER_MAP: &[(char, KeyCode, bool)] = &[
    ('1', KEY_1, false),
    ('!', KEY_1, true),
    ('2', KEY_2, false),
    ('@', KEY_2, true),
    ('3', KEY_3, false),
    ('#', KEY_3, true),
    ('4', KEY_4, false),
    ('$', KEY_4, true),
    ('5', KEY_5, false),
    ('%', KEY_5, true),
    ('6', KEY_6, false),
    ('^', KEY_6, true),
    ('7', KEY_7, false),
    ('&', KEY_7, true),
    ('8', KEY_8, false),
    ('*', KEY_8, true),
    ('9', KEY_9, false),
    ('(', KEY_9, true),
    ('0', KEY_0, false),
    (')', KEY_0, true),
    ('`', KEY_BACKQUOTE, false),
    ('~', KEY_BACKQUOTE, true),
    ('-', KEY_MINUS, false),
    ('_', KEY_MINUS, true),
    ('=', KEY_EQUALS, false),
    ('+', KEY_EQUALS, true),
    ('[', KEY_LEFT_BRACKET, false),
    ('{', KEY_LEFT_BRACKET, true),
    (']', KEY_RIGHT_BRACKET, false),
    ('}', KEY_RIGHT_BRACKET, true),
    ('\\', KEY_BACK_SLASH, false),
    ('|', KEY_BACK_SLASH, true),
    (';', KEY_SEMICOLON, false),
    (':', KEY_SEMICOLON, true),
    ('\'', KEY_QUOTE, false),
    ('"', KEY_QUOTE, true),
    (',', KEY_COMMA, false),
    ('<', KEY_COMMA, true),
    ('.', KEY_DOT, false),
    ('>', KEY_DOT, true),
    ('/', KEY_SLASH, false),
    ('?', KEY_SLASH, true),
    (' ', KEY_SPACE, false),
];

/// Converts a character into an engine key code (with [`crate::CAPS_MASK`] for
/// upper case letters and shifted symbols).
pub fn char_to_key_code(c: char) -> Option<u32> {
    if c.is_ascii_lowercase() {
        return Some(u32::from(c.to_ascii_uppercase()));
    }
    if c.is_ascii_uppercase() {
        return Some(u32::from(c) | crate::CAPS_MASK);
    }
    CHARACTER_MAP
        .iter()
        .find(|(ch, _, _)| *ch == c)
        .map(|&(_, key, shift)| u32::from(key) | if shift { crate::CAPS_MASK } else { 0 })
}

/// Converts an engine key code (optionally with [`crate::CAPS_MASK`]) back into
/// its character. Any other flag makes the lookup fail, mirroring OpenKey's
/// `keyCodeToCharacter`.
pub fn key_code_to_char(code: u32) -> Option<char> {
    if code & !(crate::CHAR_MASK | crate::CAPS_MASK) != 0 {
        return None;
    }
    let caps = code & crate::CAPS_MASK != 0;
    let key = (code & crate::CHAR_MASK) as KeyCode;
    if is_letter(key) {
        let c = char::from(key as u8);
        return Some(if caps { c } else { c.to_ascii_lowercase() });
    }
    CHARACTER_MAP
        .iter()
        .find(|&&(_, k, shift)| k == key && shift == caps)
        .map(|&(c, _, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_and_key_code_round_trip() {
        for c in (' '..='~').filter(|c| !c.is_ascii_control()) {
            let code = char_to_key_code(c);
            assert!(code.is_some(), "missing mapping for {c:?}");
            assert_eq!(code.and_then(key_code_to_char), Some(c));
        }
    }

    #[test]
    fn key_code_with_extra_flags_has_no_character() {
        assert_eq!(key_code_to_char(u32::from(KEY_A) | crate::TONE_MASK), None);
    }
}
