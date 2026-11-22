//! User configurable options.

use crate::CodeTable;
use crate::hotkey::Hotkey;

/// Typing method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InputType {
    /// Telex (`aa` → `â`, `s` → sắc, ...).
    #[default]
    Telex,
    /// VNI (`a6` → `â`, `1` → sắc, ...).
    Vni,
    /// Telex without `w` → `ư` and without `[`/`]`.
    SimpleTelex1,
    /// Telex without `[`/`]`.
    SimpleTelex2,
}

impl InputType {
    /// All input types in OpenKey's numbering order.
    pub const ALL: [InputType; 4] = [
        InputType::Telex,
        InputType::Vni,
        InputType::SimpleTelex1,
        InputType::SimpleTelex2,
    ];

    /// Human readable name.
    pub fn name(self) -> &'static str {
        match self {
            InputType::Telex => "Telex",
            InputType::Vni => "VNI",
            InputType::SimpleTelex1 => "Simple Telex 1",
            InputType::SimpleTelex2 => "Simple Telex 2",
        }
    }
}

/// All options of the input method. Defaults match OpenKey's defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Config {
    /// `true` for Vietnamese, `false` for English (pass-through).
    pub vietnamese: bool,
    /// Typing method.
    pub input_type: InputType,
    /// Output encoding.
    pub code_table: CodeTable,
    /// Allow placing tone marks anywhere ("bỏ dấu tự do").
    pub free_mark: bool,
    /// Check spelling and stop transforming invalid words.
    pub check_spelling: bool,
    /// Modern tone placement (`hoà`) instead of the classic one (`hòa`).
    pub modern_orthography: bool,
    /// Quick Telex (`cc` → `ch`, `gg` → `gi`, ...).
    pub quick_telex: bool,
    /// Restore the raw keys of an invalid word when a word break is typed.
    pub restore_if_wrong_spelling: bool,
    /// Work around browser auto-complete by sending an invisible character first.
    pub fix_recommend_browser: bool,
    /// Enable text expansion macros ("gõ tắt").
    pub use_macro: bool,
    /// Also expand macros in English mode.
    pub use_macro_in_english_mode: bool,
    /// Adapt macro output to the case of the typed abbreviation.
    pub auto_caps_macro: bool,
    /// Automatically capitalize the first letter of a sentence.
    pub upper_case_first_char: bool,
    /// Press and release Ctrl to temporarily disable spell checking.
    pub temp_off_spelling: bool,
    /// Press and release Alt to temporarily disable the engine for the current word.
    pub temp_off_engine: bool,
    /// Accept words starting with `z`, `f`, `w`, `j`.
    pub allow_consonant_zfwj: bool,
    /// Quick start consonant (`f` → `ph`, `j` → `gi`, `w` → `qu`).
    pub quick_start_consonant: bool,
    /// Quick end consonant (`g` → `ng`, `h` → `nh`, `k` → `ch`).
    pub quick_end_consonant: bool,
    /// Remember the language per application.
    pub smart_switch: bool,
    /// Remember the code table per application.
    pub remember_code_table: bool,
    /// Hotkey switching between Vietnamese and English.
    pub switch_hotkey: Hotkey,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            vietnamese: true,
            input_type: InputType::Telex,
            code_table: CodeTable::Unicode,
            free_mark: false,
            check_spelling: true,
            modern_orthography: true,
            quick_telex: false,
            restore_if_wrong_spelling: true,
            fix_recommend_browser: false,
            use_macro: true,
            use_macro_in_english_mode: true,
            auto_caps_macro: false,
            upper_case_first_char: false,
            temp_off_spelling: false,
            temp_off_engine: false,
            allow_consonant_zfwj: false,
            quick_start_consonant: false,
            quick_end_consonant: false,
            smart_switch: true,
            remember_code_table: false,
            switch_hotkey: Hotkey::default(),
        }
    }
}
