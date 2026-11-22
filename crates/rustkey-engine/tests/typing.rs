//! End-to-end typing tests: keys go through the [`Processor`] and the output is
//! replayed on a simulated text buffer.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use rustkey_engine::keys::{self, KeyCode, char_to_key_code};
use rustkey_engine::{CAPS_MASK, CodeTable, Config, InputType, KeyInput, MacroTable, Processor};

/// Types `input` and returns the resulting text. `\u{8}` is backspace.
fn type_text(processor: &mut Processor, input: &str) -> String {
    let mut text = String::new();
    type_into(processor, &mut text, input);
    text
}

fn type_into(processor: &mut Processor, text: &mut String, input: &str) {
    for c in input.chars() {
        let (key, shift) = if c == '\u{8}' {
            (keys::KEY_DELETE, false)
        } else {
            let code = char_to_key_code(c).unwrap_or_else(|| panic!("untypable {c:?}"));
            ((code & 0xFFFF) as KeyCode, code & CAPS_MASK != 0)
        };
        if shift {
            processor.handle_key(KeyInput::down(keys::KEY_LEFT_SHIFT));
        }
        let output = processor.handle_key(KeyInput::down(key));
        output.apply_to(text, key, shift);
        processor.handle_key(KeyInput::up(key));
        if shift {
            processor.handle_key(KeyInput::up(keys::KEY_LEFT_SHIFT));
        }
    }
}

fn telex() -> Processor {
    Processor::new(Config::default())
}

fn with(f: impl FnOnce(&mut Config)) -> Processor {
    let mut config = Config::default();
    f(&mut config);
    Processor::new(config)
}

#[test]
fn telex_basic_words() {
    let cases = [
        ("tieengs vieetj ", "tiếng việt "),
        ("xin chaof ", "xin chào "),
        ("dduwowngf ", "đường "),
        ("nguwowif ", "người "),
        ("Vieetj Nam ", "Việt Nam "),
        ("w ", "ư "),
        ("aw ", "ă "),
        ("dd", "đ"),
        ("thuongw ", "thương "),
        ("truwowngf ", "trường "),
        ("quaans ", "quấn "),
        ("giaf ", "già "),
        ("ddaay ", "đây "),
    ];
    for (input, expected) in cases {
        assert_eq!(type_text(&mut telex(), input), expected, "input {input:?}");
    }
}

#[test]
fn telex_double_key_undoes() {
    assert_eq!(type_text(&mut telex(), "ass "), "as ");
    assert_eq!(type_text(&mut telex(), "aaa "), "aa ");
}

#[test]
fn telex_mark_can_be_typed_after_word() {
    assert_eq!(type_text(&mut telex(), "toi "), "toi ");
    assert_eq!(type_text(&mut telex(), "tois "), "tói ");
    assert_eq!(type_text(&mut telex(), "toif "), "tòi ");
}

#[test]
fn vni_words() {
    let mut processor = with(|c| c.input_type = InputType::Vni);
    assert_eq!(type_text(&mut processor, "tie61ng vie65t "), "tiếng việt ");
    assert_eq!(type_text(&mut processor, "d9u7o7ng2 "), "đường ");
}

#[test]
fn invalid_word_is_restored() {
    // "text" is not Vietnamese: the "x" mark must be reverted on space.
    assert_eq!(type_text(&mut telex(), "text "), "text ");
    assert_eq!(type_text(&mut telex(), "windows "), "windows ");
}

#[test]
fn modern_and_classic_orthography() {
    let mut modern = telex();
    assert_eq!(type_text(&mut modern, "hoaf "), "hoà ");
    let mut classic = with(|c| c.modern_orthography = false);
    assert_eq!(type_text(&mut classic, "hoaf "), "hòa ");
}

#[test]
fn backspace_keeps_engine_in_sync() {
    assert_eq!(type_text(&mut telex(), "vieet\u{8}tj "), "việt ");
}

#[test]
fn upper_case_and_caps_lock() {
    assert_eq!(type_text(&mut telex(), "VIEETJ "), "VIỆT ");
    let mut processor = telex();
    let mut text = String::new();
    for key in [keys::KEY_D, keys::KEY_D, keys::KEY_A] {
        let output = processor.handle_key(KeyInput::down(key).with_caps_lock(true));
        output.apply_to(&mut text, key, true);
    }
    assert_eq!(text, "ĐA");
}

#[test]
fn macro_is_expanded() {
    let mut processor = telex();
    let mut macros = MacroTable::new(CodeTable::Unicode);
    macros.add("vn", "Việt Nam").unwrap();
    processor.set_macros(macros);
    assert_eq!(type_text(&mut processor, "vn "), "Việt Nam ");
    assert_eq!(type_text(&mut processor, "vnx "), "vnx ");
}

#[test]
fn macro_is_expanded_in_english_mode() {
    let mut processor = with(|c| {
        c.vietnamese = false;
        c.use_macro_in_english_mode = true;
    });
    let mut macros = MacroTable::new(CodeTable::Unicode);
    macros.add("ko", "không").unwrap();
    processor.set_macros(macros);
    assert_eq!(type_text(&mut processor, "ko ok "), "không ok ");
}

#[test]
fn english_mode_passes_through() {
    let mut processor = with(|c| c.vietnamese = false);
    assert_eq!(type_text(&mut processor, "tieengs "), "tieengs ");
}

#[test]
fn hotkey_switches_language() {
    let mut processor = telex();
    processor.handle_key(KeyInput::down(keys::KEY_LEFT_CONTROL));
    processor.handle_key(KeyInput::down(keys::KEY_LEFT_SHIFT));
    let output = processor.handle_key(KeyInput::up(keys::KEY_LEFT_SHIFT));
    assert_eq!(output.language_changed, Some(false));
    processor.handle_key(KeyInput::up(keys::KEY_LEFT_CONTROL));
    assert!(!processor.config().vietnamese);
    assert_eq!(type_text(&mut processor, "as "), "as ");
}

#[test]
fn control_shortcuts_are_not_transformed() {
    let mut processor = telex();
    let mut text = String::new();
    type_into(&mut processor, &mut text, "a");
    processor.handle_key(KeyInput::down(keys::KEY_LEFT_CONTROL));
    let output = processor.handle_key(KeyInput::down(keys::KEY_S));
    assert!(!output.consumed);
    assert!(output.ops.is_empty());
    processor.handle_key(KeyInput::up(keys::KEY_S));
    processor.handle_key(KeyInput::up(keys::KEY_LEFT_CONTROL));
}

#[test]
fn vni_windows_table_tracks_two_byte_characters() {
    let mut processor = with(|c| c.code_table = CodeTable::VniWindows);
    let mut text = type_text(&mut processor, "vieetj");
    // "ệ" is two units in VNI Windows.
    assert_eq!(text.chars().count(), 5);
    type_into(&mut processor, &mut text, "\u{8}\u{8}");
    // Deleting "t" then "ệ" must remove both units of "ệ".
    assert_eq!(text, "vi");
}

#[test]
fn unicode_compound_table_uses_combining_marks() {
    let mut processor = with(|c| c.code_table = CodeTable::UnicodeCompound);
    assert_eq!(type_text(&mut processor, "vieetj "), "viê\u{323}t ");
}

#[test]
fn quick_telex() {
    let mut processor = with(|c| c.quick_telex = true);
    assert_eq!(type_text(&mut processor, "cc "), "ch ");
}

#[test]
fn mouse_click_starts_new_word() {
    let mut processor = telex();
    let mut text = type_text(&mut processor, "vie");
    processor.handle_mouse_click();
    type_into(&mut processor, &mut text, "e");
    assert_eq!(text, "viee");
}
