//! The Vietnamese typing state machine (port of OpenKey's `Engine.cpp`).
//!
//! The engine keeps the current word in a fixed size buffer where each element
//! is a key code combined with flag bits ([`crate::CAPS_MASK`],
//! [`crate::TONE_MASK`], ...). For every key event it computes a [`HookState`]
//! telling the host how many characters to delete and which characters to
//! send instead.
//!
//! The control flow intentionally mirrors the original implementation so its
//! well tested behaviour (including a few quirks) is preserved. All buffer
//! accesses are bounds checked: reads outside the buffer yield `0` and writes
//! are ignored.

use crate::code_table::{self, CodeTable};
use crate::config::{Config, InputType};
use crate::keys::*;
use crate::macros::MacroTable;
use crate::vietnamese::{
    CONSONANT_D, CONSONANT_TABLE, DOUBLE_W_ALLOWED, END_CONSONANT_TABLE, STANDALONE_W_BAD,
    VOWEL_FOR_MARK_KEYS, quick_end_consonant, quick_start_consonant, quick_telex, vowel,
    vowel_combine, vowel_for_mark,
};
use crate::{
    CAPS_MASK, CHAR_CODE_MASK, CHAR_MASK, CONSONANT_ALLOW_MASK, END_CONSONANT_MASK, MARK_MASK,
    MARK1_MASK, MARK2_MASK, MARK3_MASK, MARK4_MASK, MARK5_MASK, STANDALONE_MASK, TONE_MASK,
    TONEW_MASK,
};

/// Maximum number of characters kept for the current word.
pub const MAX_BUFF: usize = 32;
const MAX: i32 = MAX_BUFF as i32;

/// Keys that produce printable punctuation; they end a word but are remembered
/// so the word can be restored with backspace.
const CHAR_KEY_CODE: [KeyCode; 21] = [
    KEY_BACKQUOTE,
    KEY_1,
    KEY_2,
    KEY_3,
    KEY_4,
    KEY_5,
    KEY_6,
    KEY_7,
    KEY_8,
    KEY_9,
    KEY_0,
    KEY_MINUS,
    KEY_EQUALS,
    KEY_LEFT_BRACKET,
    KEY_RIGHT_BRACKET,
    KEY_BACK_SLASH,
    KEY_SEMICOLON,
    KEY_QUOTE,
    KEY_COMMA,
    KEY_DOT,
    KEY_SLASH,
];

const BREAK_CODE: [KeyCode; 30] = [
    KEY_ESC,
    KEY_TAB,
    KEY_ENTER,
    KEY_RETURN,
    KEY_LEFT,
    KEY_RIGHT,
    KEY_DOWN,
    KEY_UP,
    KEY_COMMA,
    KEY_DOT,
    KEY_SLASH,
    KEY_SEMICOLON,
    KEY_QUOTE,
    KEY_BACK_SLASH,
    KEY_MINUS,
    KEY_EQUALS,
    KEY_BACKQUOTE,
    KEY_INSERT,
    KEY_HOME,
    KEY_END,
    KEY_FORWARD_DELETE,
    KEY_PAGE_UP,
    KEY_PAGE_DOWN,
    KEY_PRINT_SCREEN,
    KEY_PRINT,
    KEY_SELECT,
    KEY_HELP,
    KEY_EXECUTE,
    KEY_NUM_LOCK,
    KEY_SCROLL_LOCK,
];

const MACRO_BREAK_CODE: [KeyCode; 9] = [
    KEY_RETURN,
    KEY_COMMA,
    KEY_DOT,
    KEY_SLASH,
    KEY_SEMICOLON,
    KEY_QUOTE,
    KEY_BACK_SLASH,
    KEY_MINUS,
    KEY_EQUALS,
];

/// Processing keys per input type: S F R X J A O E W D Z (or VNI digits).
const TELEX_KEYS: [KeyCode; 11] = [
    KEY_S, KEY_F, KEY_R, KEY_X, KEY_J, KEY_A, KEY_O, KEY_E, KEY_W, KEY_D, KEY_Z,
];
const VNI_KEYS: [KeyCode; 11] = [
    KEY_1, KEY_2, KEY_3, KEY_4, KEY_5, KEY_6, KEY_6, KEY_7, KEY_8, KEY_9, KEY_0,
];

/// What the host has to do after a key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HookCode {
    /// Let the key through unchanged.
    #[default]
    DoNothing,
    /// Delete `backspace_count` characters and send `char_data`.
    WillProcess,
    /// Reserved (unused by the engine).
    BreakWord,
    /// Like [`HookCode::WillProcess`], then also send the typed key.
    Restore,
    /// Delete `backspace_count` characters and send `macro_data`, then the typed key.
    ReplaceMacro,
    /// Like [`HookCode::Restore`] and start a new word afterwards.
    RestoreAndStartNewSession,
}

/// Kind of input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyEvent {
    /// Keyboard event.
    Keyboard,
    /// Mouse click (always ends the current word).
    Mouse,
}

/// State of an input event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyState {
    /// Key pressed.
    KeyDown,
    /// Key released.
    KeyUp,
    /// Mouse button pressed.
    MouseDown,
    /// Mouse button released.
    MouseUp,
}

/// Result of the last processed event.
#[derive(Debug, Clone, Default)]
pub struct HookState {
    /// Action required from the host.
    pub code: HookCode,
    /// Number of characters to delete.
    pub backspace_count: i32,
    /// Number of valid entries in `char_data`.
    pub new_char_count: i32,
    /// Extra information: 1 word break, 2 delete, 3 normal key, 4 no empty character needed.
    pub ext_code: u8,
    /// Characters to send, **in reverse order** (index 0 is the last character).
    pub char_data: [u32; MAX_BUFF],
    /// Keys typed since the last word break (used for macros).
    pub macro_key: Vec<u32>,
    /// Replacement data when `code` is [`HookCode::ReplaceMacro`].
    pub macro_data: Vec<u32>,
}

/// Converts an engine value into its encoded character for `table`.
///
/// Values without a matching character are returned unchanged; encoded values
/// carry [`CHAR_CODE_MASK`].
pub(crate) fn character_code(data: u32, table: CodeTable) -> u32 {
    let caps_elem: usize = if data & CAPS_MASK != 0 { 0 } else { 1 };
    let mut key = data & CHAR_MASK;
    if data & MARK_MASK != 0 {
        let base: i32 = match data & MARK_MASK {
            MARK1_MASK => 0,
            MARK2_MASK => 2,
            MARK3_MASK => 4,
            MARK4_MASK => 6,
            MARK5_MASK => 8,
            _ => -2,
        };
        let mut mark_elem = base + caps_elem as i32;
        let plain_vowel = [KEY_A, KEY_O, KEY_U, KEY_E]
            .iter()
            .any(|&k| u32::from(k) == key);
        if plain_vowel && data & (TONE_MASK | TONEW_MASK) == 0 {
            mark_elem += 4;
        }
        if data & TONE_MASK != 0 {
            key |= TONE_MASK;
        } else if data & TONEW_MASK != 0 {
            key |= TONEW_MASK;
        }
        let Some(row) = code_table::lookup(table, key) else {
            return data;
        };
        usize::try_from(mark_elem)
            .ok()
            .and_then(|index| row.get(index))
            .map_or(data, |&c| u32::from(c) | CHAR_CODE_MASK)
    } else {
        let Some(row) = code_table::lookup(table, key) else {
            return data;
        };
        let index = if data & TONE_MASK != 0 {
            caps_elem
        } else if data & TONEW_MASK != 0 {
            caps_elem + 2
        } else {
            return data;
        };
        row.get(index)
            .map_or(data, |&c| u32::from(c) | CHAR_CODE_MASK)
    }
}

fn is_consonant(key: u16) -> bool {
    !matches!(key, KEY_A | KEY_E | KEY_U | KEY_Y | KEY_I | KEY_O)
}

fn is_number_key(key: u16) -> bool {
    is_number(key)
}

fn is_bracket_key(key: u16) -> bool {
    key == KEY_LEFT_BRACKET || key == KEY_RIGHT_BRACKET
}

fn is_char_key_code(key: u16) -> bool {
    CHAR_KEY_CODE.contains(&key)
}

fn is_macro_break_code(key: u16) -> bool {
    MACRO_BREAK_CODE.contains(&key)
}

fn is_word_break(event: KeyEvent, key: u16) -> bool {
    event == KeyEvent::Mouse || BREAK_CODE.contains(&key)
}

fn caps_bit(caps: bool) -> u32 {
    if caps { CAPS_MASK } else { 0 }
}

/// The Vietnamese typing engine.
#[derive(Debug, Clone)]
pub struct Engine {
    config: Config,
    macros: MacroTable,
    /// Runtime spell checking flag (may be toggled temporarily).
    spell_check: bool,
    use_spell_checking_before: bool,

    hs: HookState,

    tw: [u32; MAX_BUFF],
    index: i32,
    long_word_helper: Vec<u32>,
    typing_states: Vec<Vec<u32>>,
    key_states: [u32; MAX_BUFF],
    state_index: i32,

    temp_disable_key: bool,
    is_changed: bool,
    is_restored_w: bool,
    is_checked_grammar: bool,
    is_caps: bool,
    vowel_count: i32,
    vsi: i32,
    vei: i32,
    vwsm: i32,
    space_count: i32,
    has_handled_macro: bool,
    upper_case_status: u8,
    special_char: Vec<u32>,
    has_handle_quick_consonant: bool,
    will_temp_off_engine: bool,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Engine {
    /// Creates an engine with the given configuration and no macros.
    pub fn new(config: Config) -> Self {
        let macros = MacroTable::new(config.code_table);
        Self {
            spell_check: config.check_spelling,
            use_spell_checking_before: config.check_spelling,
            config,
            macros,
            hs: HookState::default(),
            tw: [0; MAX_BUFF],
            index: 0,
            long_word_helper: Vec::new(),
            typing_states: Vec::new(),
            key_states: [0; MAX_BUFF],
            state_index: 0,
            temp_disable_key: false,
            is_changed: false,
            is_restored_w: false,
            is_checked_grammar: false,
            is_caps: false,
            vowel_count: 0,
            vsi: 0,
            vei: 0,
            vwsm: 0,
            space_count: 0,
            has_handled_macro: false,
            upper_case_status: 0,
            special_char: Vec::new(),
            has_handle_quick_consonant: false,
            will_temp_off_engine: false,
        }
    }

    /// Current configuration.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Replaces the configuration, updating dependent state.
    pub fn set_config(&mut self, config: Config) {
        let table_changed = config.code_table != self.config.code_table;
        let spelling_changed = config.check_spelling != self.config.check_spelling;
        let input_changed = config.input_type != self.config.input_type;
        self.config = config;
        if table_changed {
            self.macros.set_code_table(self.config.code_table);
        }
        if spelling_changed {
            self.spell_check = self.config.check_spelling;
            self.use_spell_checking_before = self.config.check_spelling;
        }
        if table_changed || input_changed {
            self.start_new_session();
        }
    }

    /// Macro table used for text expansion.
    pub fn macros(&self) -> &MacroTable {
        &self.macros
    }

    /// Replaces the macro table (its code table is adapted to the configuration).
    pub fn set_macros(&mut self, mut macros: MacroTable) {
        macros.set_code_table(self.config.code_table);
        self.macros = macros;
    }

    /// Mutable access to the macro table.
    pub fn macros_mut(&mut self) -> &mut MacroTable {
        &mut self.macros
    }

    /// Result of the last event.
    pub fn state(&self) -> &HookState {
        &self.hs
    }

    /// Whether spell checking is currently active.
    pub fn is_spell_checking(&self) -> bool {
        self.spell_check
    }

    /// Encodes an engine value using the configured code table.
    pub fn character_code(&self, data: u32) -> u32 {
        character_code(data, self.config.code_table)
    }

    /// Toggles spell checking until the next word break.
    pub fn temp_off_spell_checking(&mut self) {
        if self.use_spell_checking_before {
            self.spell_check = !self.spell_check;
        }
    }

    /// Disables the engine until the next word break.
    pub fn temp_off_engine(&mut self, off: bool) {
        self.will_temp_off_engine = off;
    }

    /// Forgets the current word.
    pub fn start_new_session(&mut self) {
        self.index = 0;
        self.hs.backspace_count = 0;
        self.hs.new_char_count = 0;
        self.temp_disable_key = false;
        self.state_index = 0;
        self.has_handled_macro = false;
        self.has_handle_quick_consonant = false;
        self.long_word_helper.clear();
    }

    // ---------------------------------------------------------------------
    // Buffer helpers
    // ---------------------------------------------------------------------

    fn chr(&self, i: i32) -> u16 {
        self.tw(i) as u16
    }

    fn tw(&self, i: i32) -> u32 {
        usize::try_from(i)
            .ok()
            .and_then(|i| self.tw.get(i))
            .copied()
            .unwrap_or(0)
    }

    fn tw_mut(&mut self, i: i32) -> Option<&mut u32> {
        usize::try_from(i).ok().and_then(|i| self.tw.get_mut(i))
    }

    fn tw_set(&mut self, i: i32, value: u32) {
        if let Some(slot) = self.tw_mut(i) {
            *slot = value;
        }
    }

    fn tw_or(&mut self, i: i32, mask: u32) {
        if let Some(slot) = self.tw_mut(i) {
            *slot |= mask;
        }
    }

    fn tw_clear(&mut self, i: i32, mask: u32) {
        if let Some(slot) = self.tw_mut(i) {
            *slot &= !mask;
        }
    }

    fn set_data(&mut self, i: i32, value: u32) {
        if let Some(slot) = usize::try_from(i)
            .ok()
            .and_then(|i| self.hs.char_data.get_mut(i))
        {
            *slot = value;
        }
    }

    fn get(&self, data: u32) -> u32 {
        character_code(data, self.config.code_table)
    }

    // ---------------------------------------------------------------------
    // Input type helpers
    // ---------------------------------------------------------------------

    fn is_vni(&self) -> bool {
        self.config.input_type == InputType::Vni
    }

    fn processing(&self, index: usize) -> u16 {
        if self.is_vni() {
            VNI_KEYS[index]
        } else {
            TELEX_KEYS[index]
        }
    }

    fn is_key_z(&self, key: u16) -> bool {
        self.processing(10) == key
    }

    fn is_key_d(&self, key: u16) -> bool {
        self.processing(9) == key
    }

    fn is_key_w(&self, key: u16) -> bool {
        if self.is_vni() {
            self.processing(8) == key || self.processing(7) == key
        } else {
            self.processing(8) == key
        }
    }

    fn is_key_double(&self, key: u16) -> bool {
        if self.is_vni() {
            self.processing(6) == key
        } else {
            self.processing(5) == key || self.processing(6) == key || self.processing(7) == key
        }
    }

    fn is_mark_key(&self, key: u16) -> bool {
        if self.is_vni() {
            matches!(key, KEY_1 | KEY_2 | KEY_3 | KEY_4 | KEY_5)
        } else {
            matches!(key, KEY_S | KEY_F | KEY_R | KEY_J | KEY_X)
        }
    }

    fn is_special_key(&self, key: u16) -> bool {
        match self.config.input_type {
            InputType::Telex => matches!(
                key,
                KEY_W
                    | KEY_E
                    | KEY_R
                    | KEY_O
                    | KEY_LEFT_BRACKET
                    | KEY_RIGHT_BRACKET
                    | KEY_A
                    | KEY_S
                    | KEY_D
                    | KEY_F
                    | KEY_J
                    | KEY_Z
                    | KEY_X
            ),
            InputType::Vni => is_number(key),
            InputType::SimpleTelex1 | InputType::SimpleTelex2 => matches!(
                key,
                KEY_W | KEY_E | KEY_R | KEY_O | KEY_A | KEY_S | KEY_D | KEY_F | KEY_J | KEY_Z | KEY_X
            ),
        }
    }

    fn is_quick_telex_key(&self, key: u16) -> bool {
        self.index > 0
            && matches!(key, KEY_C | KEY_G | KEY_K | KEY_N | KEY_Q | KEY_P | KEY_T)
            && self.chr(self.index - 1) == key
    }

    fn end_consonant_strip(&self) -> u16 {
        if self.config.quick_end_consonant {
            END_CONSONANT_MASK as u16
        } else {
            0
        }
    }

    // ---------------------------------------------------------------------
    // Spelling
    // ---------------------------------------------------------------------

    fn check_spelling(&mut self, force_check_vowel: bool) {
        let mut ok = false;
        let mut vowel_ok = true;
        let mut end = self.index;
        if self.index > 0 && self.chr(self.index - 1) == KEY_RIGHT_BRACKET {
            end = self.index - 1;
        }

        if end > 0 {
            let mut j: i32 = 0;
            if is_consonant(self.chr(0)) {
                let start_strip = if self.config.quick_start_consonant {
                    END_CONSONANT_MASK as u16
                } else {
                    0
                };
                let allow_strip = if self.config.allow_consonant_zfwj {
                    CONSONANT_ALLOW_MASK as u16
                } else {
                    0
                };
                for entry in CONSONANT_TABLE {
                    let mut flag = (end as usize) < entry.len();
                    j = 0;
                    while (j as usize) < entry.len() {
                        let e = entry[j as usize];
                        let c = self.chr(j);
                        if end > j && (e & !start_strip) != c && (e & !allow_strip) != c {
                            flag = true;
                            break;
                        }
                        j += 1;
                    }
                    if flag {
                        continue;
                    }
                    break;
                }
            }

            if j == end {
                ok = true;
            }

            let mut k = j;
            self.vsi = k;
            if self.chr(self.vsi) == KEY_U
                && k > 0
                && k < end - 1
                && self.chr(self.vsi - 1) == KEY_Q
            {
                k += 1;
                j = k;
                self.vsi = k;
            } else if self.index >= 2
                && self.chr(0) == KEY_G
                && self.chr(1) == KEY_I
                && is_consonant(self.chr(2))
            {
                self.vsi = 1;
                k = 1;
                j = 1;
            }
            for _ in 0..3 {
                if k < end && !is_consonant(self.chr(k)) {
                    k += 1;
                    self.vei = k;
                }
            }

            if k > j {
                vowel_ok = false;
                if k - j > 1 && force_check_vowel {
                    for combo in vowel_combine(self.chr(j)) {
                        let mut flag = false;
                        let mut ii: i32 = 1;
                        while (ii as usize) < combo.len() {
                            let p = j + ii - 1;
                            let value = u32::from(self.chr(p))
                                | (self.tw(p) & TONEW_MASK)
                                | (self.tw(p) & TONE_MASK);
                            if p < end && combo[ii as usize] != value {
                                flag = true;
                                break;
                            }
                            ii += 1;
                        }
                        let p = j + ii - 1;
                        if flag
                            || (k < end && combo.first().copied().unwrap_or(0) == 0)
                            || (p < end && !is_consonant(self.chr(p)))
                        {
                            continue;
                        }
                        vowel_ok = true;
                        break;
                    }
                } else if !is_consonant(self.chr(j)) {
                    vowel_ok = true;
                }

                let strip = self.end_consonant_strip();
                for entry in END_CONSONANT_TABLE {
                    let mut flag = false;
                    let mut jj: i32 = 0;
                    while (jj as usize) < entry.len() {
                        if end > k + jj && (entry[jj as usize] & !strip) != self.chr(k + jj) {
                            flag = true;
                            break;
                        }
                        jj += 1;
                    }
                    if flag {
                        continue;
                    }
                    if k + jj >= end {
                        ok = true;
                        break;
                    }
                }

                // End consonants "ch" and "t" only accept sắc, nặng or no mark.
                if ok {
                    let allowed = |value: u32| {
                        value & MARK1_MASK != 0
                            || value & MARK5_MASK != 0
                            || value & MARK_MASK == 0
                    };
                    let idx = self.index;
                    let ch_invalid = idx >= 3
                        && self.chr(idx - 1) == KEY_H
                        && self.chr(idx - 2) == KEY_C
                        && !allowed(self.tw(idx - 3));
                    let t_invalid =
                        idx >= 2 && self.chr(idx - 1) == KEY_T && !allowed(self.tw(idx - 2));
                    if ch_invalid || t_invalid {
                        ok = false;
                    }
                }
            }
        } else {
            ok = true;
        }
        self.temp_disable_key = !(ok && vowel_ok);
    }

    fn check_grammar(&mut self, delta_backspace: i32) {
        if self.index <= 1 || self.index >= MAX {
            return;
        }
        self.find_and_calculate_vowel(true);
        if self.vowel_count == 0 {
            return;
        }
        self.is_checked_grammar = false;
        let l = self.vsi;

        // "thuơn", "ưoi", "ưom", "ưoc" → "ươ"
        if self.index >= 3 {
            for i in (0..self.index).rev() {
                if matches!(
                    self.chr(i),
                    KEY_N | KEY_C | KEY_I | KEY_M | KEY_P | KEY_T
                ) && i >= 2
                    && self.chr(i - 1) == KEY_O
                    && self.chr(i - 2) == KEY_U
                    && (self.tw(i - 1) & TONEW_MASK) ^ (self.tw(i - 2) & TONEW_MASK) != 0
                {
                    self.tw_or(i - 2, TONEW_MASK);
                    self.tw_or(i - 1, TONEW_MASK);
                    self.is_checked_grammar = true;
                    break;
                }
            }
        }

        // Move the tone mark to the right vowel.
        if self.index >= 2 {
            let mut i = l;
            while i <= self.vei {
                let mark = self.tw(i) & MARK_MASK;
                if mark != 0 {
                    self.tw_clear(i, MARK_MASK);
                    self.insert_mark(mark, false);
                    if i != self.vwsm {
                        self.is_checked_grammar = true;
                    }
                    break;
                }
                i += 1;
            }
        }

        if self.is_checked_grammar {
            if self.hs.code == HookCode::DoNothing {
                self.hs.code = HookCode::WillProcess;
            }
            self.hs.backspace_count = 0;
            for i in (l..self.index).rev() {
                self.hs.backspace_count += 1;
                let value = self.get(self.tw(i));
                self.set_data(self.index - 1 - i, value);
            }
            self.hs.new_char_count = self.hs.backspace_count;
            self.hs.backspace_count += delta_backspace;
            self.hs.ext_code = 4;
        }
    }

    fn insert_key(&mut self, key: u16, caps: bool, check_spelling: bool) {
        let value = u32::from(key) | caps_bit(caps);
        if self.index >= MAX {
            self.long_word_helper.push(self.tw[0]);
            self.tw.copy_within(1.., 0);
            self.tw_set(self.index - 1, value);
        } else {
            self.tw_set(self.index, value);
            self.index += 1;
        }

        if self.spell_check && check_spelling {
            self.check_spelling(false);
        }

        // Allow "d" after a consonant.
        if key == KEY_D && self.index >= 2 && is_consonant(self.chr(self.index - 2)) {
            self.temp_disable_key = false;
        }
    }

    fn insert_state(&mut self, key: u16, caps: bool) {
        let value = u32::from(key) | caps_bit(caps);
        if self.state_index >= MAX {
            self.key_states.copy_within(1.., 0);
            self.key_states[MAX_BUFF - 1] = value;
        } else if let Some(slot) = usize::try_from(self.state_index)
            .ok()
            .and_then(|i| self.key_states.get_mut(i))
        {
            *slot = value;
            self.state_index += 1;
        }
    }

    fn save_word(&mut self) {
        if self.hs.code != HookCode::ReplaceMacro {
            if self.index > 0 {
                if !self.long_word_helper.is_empty() {
                    for chunk in self.long_word_helper.chunks(MAX_BUFF) {
                        self.typing_states.push(chunk.to_vec());
                    }
                    self.long_word_helper.clear();
                }
                let len = (self.index as usize).min(MAX_BUFF);
                self.typing_states.push(self.tw[..len].to_vec());
            }
        } else if self.hs.macro_data.is_empty() {
            self.typing_states.push(Vec::new());
        } else {
            for chunk in self.hs.macro_data.chunks(MAX_BUFF) {
                self.typing_states.push(chunk.to_vec());
            }
        }
    }

    fn save_word_repeated(&mut self, key: u32, count: i32) {
        let count = usize::try_from(count).unwrap_or(0);
        self.typing_states.push(vec![key; count]);
    }

    fn save_special_char(&mut self) {
        let data = std::mem::take(&mut self.special_char);
        self.typing_states.push(data);
    }

    fn restore_last_typing_state(&mut self) {
        let Some(data) = self.typing_states.pop() else {
            return;
        };
        let Some(&first) = data.first() else {
            return;
        };
        if first == u32::from(KEY_SPACE) {
            self.space_count = data.len() as i32;
            self.index = 0;
        } else if is_char_key_code(first as u16) {
            self.index = 0;
            self.special_char = data;
            self.check_spelling(false);
        } else {
            let len = data.len().min(MAX_BUFF);
            self.tw[..len].copy_from_slice(&data[..len]);
            self.index = len as i32;
        }
    }

    fn check_correct_vowel(&self, pattern: &[u16], mark_key: u16) -> bool {
        // Ignore the "qu" case.
        if self.index >= 2 && self.chr(self.index - 1) == KEY_U && self.chr(self.index - 2) == KEY_Q {
            return false;
        }
        let strip = self.end_consonant_strip();
        let mut k = self.index - 1;
        for &p in pattern.iter().rev() {
            if (p & !strip) != self.chr(k) {
                return false;
            }
            k -= 1;
            if k < 0 {
                break;
            }
        }

        // Limit marks for end consonants "c" and "t".
        let is_f = self.processing(1) == mark_key;
        let is_r = self.processing(2) == mark_key;
        let is_x = self.processing(3) == mark_key;
        if pattern.len() > 1 && (is_f || is_x || is_r) {
            if pattern[1] == KEY_C || pattern[1] == KEY_T {
                return false;
            }
            if pattern.len() > 2 && pattern[2] == KEY_T {
                return false;
            }
        }

        !(k >= 0 && self.chr(k) == self.chr(k + 1))
    }

    fn find_and_calculate_vowel(&mut self, for_grammar: bool) {
        self.vowel_count = 0;
        self.vsi = 0;
        self.vei = 0;
        let mut i = self.index - 1;
        while i >= 0 {
            if is_consonant(self.chr(i)) {
                if self.vowel_count > 0 {
                    break;
                }
            } else {
                if self.vowel_count == 0 {
                    self.vei = i;
                }
                if !for_grammar
                    && i >= 1
                    && ((self.chr(i) == KEY_I && self.chr(i - 1) == KEY_G)
                        || (self.chr(i) == KEY_U && self.chr(i - 1) == KEY_Q))
                {
                    break;
                }
                self.vsi = i;
                self.vowel_count += 1;
            }
            i -= 1;
        }
        // Don't count "u" in "qu" as a vowel.
        if self.vsi >= 1 && self.chr(self.vsi) == KEY_U && self.chr(self.vsi - 1) == KEY_Q {
            self.vsi += 1;
            self.vowel_count -= 1;
        }
    }

    fn remove_mark(&mut self) {
        self.find_and_calculate_vowel(true);
        self.is_changed = false;
        if self.index > 0 {
            for i in self.vsi..=self.vei {
                if self.tw(i) & MARK_MASK != 0 {
                    self.tw_clear(i, MARK_MASK);
                    self.is_changed = true;
                }
            }
        }
        if self.is_changed {
            self.hs.code = HookCode::WillProcess;
            self.hs.backspace_count = 0;
            for i in (self.vsi..self.index).rev() {
                self.hs.backspace_count += 1;
                let value = self.get(self.tw(i));
                self.set_data(self.index - 1 - i, value);
            }
            self.hs.new_char_count = self.hs.backspace_count;
        } else {
            self.hs.code = HookCode::DoNothing;
        }
    }

    fn can_has_end_consonant(&self) -> bool {
        for combo in vowel_combine(self.chr(self.vsi)) {
            let mut kk = self.vsi;
            let mut iii = 1;
            while iii < combo.len() {
                let value =
                    u32::from(self.chr(kk)) | (self.tw(kk) & TONE_MASK) | (self.tw(kk) & TONEW_MASK);
                if kk > self.vei || value != combo[iii] {
                    break;
                }
                kk += 1;
                iii += 1;
            }
            if iii >= combo.len() {
                return combo.first().copied() == Some(1);
            }
        }
        false
    }

    fn handle_modern_mark(&mut self) {
        let vsi = self.vsi;
        let vei = self.vei;
        let index = self.index;
        let c = |i: i32| self.chr(i);
        let t = |i: i32| self.tw(i);

        // Default.
        let mut vwsm = vei;

        // Rule 2.
        if self.vowel_count == 3
            && ((c(vsi) == KEY_O && c(vsi + 1) == KEY_A && c(vsi + 2) == KEY_I)
                || (c(vsi) == KEY_U && c(vsi + 1) == KEY_Y && c(vsi + 2) == KEY_U)
                || (c(vsi) == KEY_O && c(vsi + 1) == KEY_E && c(vsi + 2) == KEY_O)
                || (c(vsi) == KEY_U && c(vsi + 1) == KEY_Y && c(vsi + 2) == KEY_A))
        {
            vwsm = vsi + 1;
        } else if (c(vsi) == KEY_O && c(vsi + 1) == KEY_I)
            || (c(vsi) == KEY_A && c(vsi + 1) == KEY_I)
            || (c(vsi) == KEY_U && c(vsi + 1) == KEY_I)
        {
            vwsm = vsi;
        } else if c(vei - 1) == KEY_A && c(vei) == KEY_Y {
            vwsm = vei - 1;
        } else if c(vsi) == KEY_U && c(vsi + 1) == KEY_O {
            vwsm = vsi + 1;
        } else if c(vsi + 1) == KEY_O || c(vsi + 1) == KEY_U {
            vwsm = vei - 1;
        } else if c(vsi) == KEY_O || c(vsi) == KEY_U {
            vwsm = vei;
        }

        // Rule 3.1 (the bitwise test on "e" mirrors the original implementation).
        let e_or_tone = u32::from(KEY_E) | TONE_MASK;
        if (c(vsi) == KEY_I && t(vsi + 1) & e_or_tone != 0)
            || (c(vsi) == KEY_Y && t(vsi + 1) & e_or_tone != 0)
            || (c(vsi) == KEY_U && t(vsi + 1) == (u32::from(KEY_O) | TONE_MASK))
            || (t(vsi) == (u32::from(KEY_U) | TONEW_MASK)
                && t(vsi + 1) == (u32::from(KEY_O) | TONEW_MASK))
        {
            vwsm = if vsi + 2 < index
                && matches!(
                    c(vsi + 2),
                    KEY_P | KEY_T | KEY_M | KEY_N | KEY_O | KEY_U | KEY_I | KEY_C
                ) {
                vsi + 1
            } else {
                vsi
            };
        }
        // Rule 3.2
        else if c(vsi) == KEY_U && t(vsi + 1) == (u32::from(KEY_U) | TONEW_MASK) {
            vwsm = vsi;
        }

        // Rule 4.
        if self.vowel_count == 2 {
            if c(vsi) == KEY_I && matches!(c(vsi + 1), KEY_A | KEY_U | KEY_O) {
                vwsm = if vsi == 0 || c(vsi - 1) != KEY_G {
                    vsi
                } else {
                    vsi + 1
                };
            } else if c(vsi) == KEY_U && c(vsi + 1) == KEY_A {
                if vsi == 0 || c(vsi - 1) != KEY_Q {
                    if vei + 1 >= index || !self.can_has_end_consonant() {
                        vwsm = vsi;
                    }
                } else {
                    vwsm = vsi + 1;
                }
            } else if c(vsi) == KEY_O && c(vsi + 1) == KEY_O {
                vwsm = vei;
            }
        }

        self.vwsm = vwsm;
        self.hs.backspace_count = index - vwsm;
    }

    fn handle_old_mark(&mut self) {
        let vsi = self.vsi;
        let vei = self.vei;
        let index = self.index;

        let mut vwsm = if self.vowel_count == 0 && self.chr(vei) == KEY_I {
            vei
        } else {
            vsi
        };

        if self.vowel_count == 3
            || (vei + 1 < index && is_consonant(self.chr(vei + 1)) && self.can_has_end_consonant())
        {
            vwsm = vsi + 1;
        }

        for ii in vsi..=vei {
            if (self.chr(ii) == KEY_E && self.tw(ii) & TONE_MASK != 0)
                || (self.chr(ii) == KEY_O && self.tw(ii) & TONEW_MASK != 0)
            {
                vwsm = ii;
                break;
            }
        }

        self.vwsm = vwsm;
        self.hs.backspace_count = index - vwsm;
        self.hs.new_char_count = self.hs.backspace_count;
    }

    fn insert_mark(&mut self, mark_mask: u32, can_modify_flag: bool) {
        self.vowel_count = 0;
        if can_modify_flag {
            self.hs.code = HookCode::WillProcess;
        }
        self.hs.backspace_count = 0;
        self.hs.new_char_count = 0;

        self.find_and_calculate_vowel(false);
        self.vwsm = 0;

        if self.vowel_count == 1 {
            self.vwsm = self.vei;
            self.hs.backspace_count = self.index - self.vei;
        } else {
            if self.config.modern_orthography {
                self.handle_modern_mark();
            } else {
                self.handle_old_mark();
            }
            if self.tw(self.vei) & (TONE_MASK | TONEW_MASK) != 0 {
                self.vwsm = self.vei;
            }
        }

        let mut kk = self.index - 1 - self.vsi;
        if self.tw(self.vwsm) & mark_mask != 0 {
            // Same mark typed twice: remove it.
            self.tw_clear(self.vwsm, MARK_MASK);
            if can_modify_flag {
                self.hs.code = HookCode::Restore;
            }
            for ii in self.vsi..self.index {
                self.tw_clear(ii, MARK_MASK);
                let value = self.get(self.tw(ii));
                self.set_data(kk, value);
                kk -= 1;
            }
            self.temp_disable_key = true;
        } else {
            self.tw_clear(self.vwsm, MARK_MASK);
            self.tw_or(self.vwsm, mark_mask);
            for ii in self.vsi..self.index {
                if ii != self.vwsm {
                    self.tw_clear(ii, MARK_MASK);
                }
                let value = self.get(self.tw(ii));
                self.set_data(kk, value);
                kk -= 1;
            }
            self.hs.backspace_count = self.index - self.vsi;
        }
        self.hs.new_char_count = self.hs.backspace_count;
    }

    fn insert_d(&mut self) {
        self.hs.code = HookCode::WillProcess;
        self.hs.backspace_count = 0;
        let index = self.index;
        let mut ii = index - 1;
        while ii >= 0 {
            self.hs.backspace_count += 1;
            if self.chr(ii) == KEY_D {
                if self.tw(ii) & TONE_MASK != 0 {
                    self.hs.code = HookCode::Restore;
                    self.tw_clear(ii, TONE_MASK);
                    self.set_data(index - 1 - ii, self.tw(ii));
                    self.temp_disable_key = true;
                } else {
                    self.tw_or(ii, TONE_MASK);
                    let value = self.get(self.tw(ii));
                    self.set_data(index - 1 - ii, value);
                }
                break;
            }
            let value = self.get(self.tw(ii));
            self.set_data(index - 1 - ii, value);
            ii -= 1;
        }
        self.hs.new_char_count = self.hs.backspace_count;
    }

    fn insert_aoe(&mut self, data: u16) {
        self.find_and_calculate_vowel(false);
        for ii in self.vsi..=self.vei {
            self.tw_clear(ii, TONEW_MASK);
        }

        self.hs.code = HookCode::WillProcess;
        self.hs.backspace_count = 0;
        let index = self.index;
        let mut ii = index - 1;
        while ii >= 0 {
            self.hs.backspace_count += 1;
            if self.chr(ii) == data {
                if self.tw(ii) & TONE_MASK != 0 {
                    self.hs.code = HookCode::Restore;
                    self.tw_clear(ii, TONE_MASK);
                    self.set_data(index - 1 - ii, self.tw(ii));
                    if data != KEY_O {
                        // Keep "thoòng" working.
                        self.temp_disable_key = true;
                    }
                } else {
                    self.tw_or(ii, TONE_MASK);
                    if !self.is_key_d(data) {
                        self.tw_clear(ii, TONEW_MASK);
                    }
                    let value = self.get(self.tw(ii));
                    self.set_data(index - 1 - ii, value);
                }
                break;
            }
            let value = self.get(self.tw(ii));
            self.set_data(index - 1 - ii, value);
            ii -= 1;
        }
        self.hs.new_char_count = self.hs.backspace_count;
    }

    fn insert_w(&mut self) {
        self.is_restored_w = false;
        self.find_and_calculate_vowel(false);

        for ii in self.vsi..=self.vei {
            self.tw_clear(ii, TONE_MASK);
        }

        let vsi = self.vsi;
        let index = self.index;

        if self.vowel_count > 1 {
            self.hs.backspace_count = index - vsi;
            self.hs.new_char_count = self.hs.backspace_count;

            let first_w = self.tw(vsi) & TONEW_MASK != 0;
            if (first_w && self.tw(vsi + 1) & TONEW_MASK != 0)
                || (first_w && self.chr(vsi + 1) == KEY_I)
                || (first_w && self.chr(vsi + 1) == KEY_A)
            {
                self.hs.code = HookCode::Restore;
                for ii in vsi..index {
                    self.tw_clear(ii, TONEW_MASK);
                    let value = self.get(self.tw(ii)) & !STANDALONE_MASK;
                    self.set_data(index - 1 - ii, value);
                }
                self.is_restored_w = true;
                self.temp_disable_key = true;
            } else {
                self.hs.code = HookCode::WillProcess;
                let c0 = self.chr(vsi);
                let c1 = self.chr(vsi + 1);
                if c0 == KEY_U && c1 == KEY_O {
                    if vsi >= 2
                        && self.tw(vsi - 2) == u32::from(KEY_T)
                        && self.tw(vsi - 1) == u32::from(KEY_H)
                    {
                        self.tw_or(vsi + 1, TONEW_MASK);
                        if vsi + 2 < index && self.chr(vsi + 2) == KEY_N {
                            self.tw_or(vsi, TONEW_MASK);
                        }
                    } else if vsi >= 1 && self.tw(vsi - 1) == u32::from(KEY_Q) {
                        self.tw_or(vsi + 1, TONEW_MASK);
                    } else {
                        self.tw_or(vsi, TONEW_MASK);
                        self.tw_or(vsi + 1, TONEW_MASK);
                    }
                } else if (c0 == KEY_U && matches!(c1, KEY_A | KEY_I | KEY_U))
                    || (c0 == KEY_O && c1 == KEY_I)
                {
                    self.tw_or(vsi, TONEW_MASK);
                } else if (c0 == KEY_I && c1 == KEY_O) || (c0 == KEY_O && c1 == KEY_A) {
                    self.tw_or(vsi + 1, TONEW_MASK);
                } else {
                    self.temp_disable_key = true;
                    self.is_changed = false;
                    self.hs.code = HookCode::DoNothing;
                }

                for ii in vsi..index {
                    let value = self.get(self.tw(ii));
                    self.set_data(index - 1 - ii, value);
                }
            }
            return;
        }

        self.hs.code = HookCode::WillProcess;
        self.hs.backspace_count = 0;
        let mut ii = index - 1;
        while ii >= 0 {
            if ii < vsi {
                break;
            }
            self.hs.backspace_count += 1;
            match self.chr(ii) {
                KEY_A | KEY_U | KEY_O => {
                    if self.tw(ii) & TONEW_MASK != 0 {
                        let caps = self.tw(ii) & CAPS_MASK;
                        if self.tw(ii) & STANDALONE_MASK != 0 {
                            self.hs.code = HookCode::WillProcess;
                            if self.chr(ii) == KEY_U {
                                self.tw_set(ii, u32::from(KEY_W) | caps);
                            } else if self.chr(ii) == KEY_O {
                                self.hs.code = HookCode::Restore;
                                self.tw_set(ii, u32::from(KEY_O) | caps);
                                self.is_restored_w = true;
                            }
                            self.set_data(index - 1 - ii, self.tw(ii));
                        } else {
                            self.hs.code = HookCode::Restore;
                            self.tw_clear(ii, TONEW_MASK);
                            self.set_data(index - 1 - ii, self.tw(ii));
                            self.is_restored_w = true;
                        }
                        self.temp_disable_key = true;
                    } else {
                        self.tw_or(ii, TONEW_MASK);
                        self.tw_clear(ii, TONE_MASK);
                        let value = self.get(self.tw(ii));
                        self.set_data(index - 1 - ii, value);
                    }
                }
                _ => {
                    let value = self.get(self.tw(ii));
                    self.set_data(index - 1 - ii, value);
                }
            }
            ii -= 1;
        }
        self.hs.new_char_count = self.hs.backspace_count;
    }

    fn reverse_last_standalone_char(&mut self, key: u16, caps: bool) {
        self.hs.code = HookCode::WillProcess;
        self.hs.backspace_count = 0;
        self.hs.new_char_count = 1;
        self.hs.ext_code = 4;
        let last = self.index - 1;
        self.tw_set(
            last,
            u32::from(key) | TONEW_MASK | STANDALONE_MASK | caps_bit(caps),
        );
        let value = self.get(self.tw(last));
        self.set_data(0, value);
    }

    fn check_for_standalone_char(&mut self, data: u16, caps: bool, key_will_reverse: u16) {
        let last = self.index - 1;
        if self.chr(last) == key_will_reverse && self.tw(last) & TONEW_MASK != 0 {
            self.hs.code = HookCode::WillProcess;
            self.hs.backspace_count = 1;
            self.hs.new_char_count = 1;
            self.tw_set(last, u32::from(data) | caps_bit(caps));
            let value = self.get(self.tw(last));
            self.set_data(0, value);
            return;
        }

        // Standalone "w" → "ư".
        if self.index > 0 && self.chr(last) == KEY_U && key_will_reverse == KEY_O {
            self.insert_key(key_will_reverse, caps, true);
            self.reverse_last_standalone_char(key_will_reverse, caps);
            return;
        }

        match self.index {
            0 => {
                self.insert_key(data, caps, false);
                self.reverse_last_standalone_char(key_will_reverse, caps);
            }
            1 => {
                if STANDALONE_W_BAD.contains(&self.chr(0)) {
                    self.insert_key(data, caps, true);
                } else {
                    self.insert_key(data, caps, false);
                    self.reverse_last_standalone_char(key_will_reverse, caps);
                }
            }
            2 => {
                let (c0, c1) = (self.chr(0), self.chr(1));
                if DOUBLE_W_ALLOWED.iter().any(|p| p[0] == c0 && p[1] == c1) {
                    self.insert_key(data, caps, false);
                    self.reverse_last_standalone_char(key_will_reverse, caps);
                } else {
                    self.insert_key(data, caps, true);
                }
            }
            _ => self.insert_key(data, caps, true),
        }
    }

    fn upper_case_first_character(&mut self) {
        if self.tw[0] & CAPS_MASK == 0 {
            self.hs.code = HookCode::WillProcess;
            self.hs.backspace_count = 0;
            self.hs.new_char_count = 1;
            self.tw[0] |= CAPS_MASK;
            let value = self.get(self.tw[0]);
            self.set_data(0, value);
            self.upper_case_status = 0;
            if self.config.use_macro
                && let Some(first) = self.hs.macro_key.first_mut()
            {
                *first |= CAPS_MASK;
            }
        }
    }

    fn handle_main_key(&mut self, data: u16, caps: bool) {
        // Z removes the mark.
        if self.is_key_z(data) {
            self.remove_mark();
            if !self.is_changed {
                self.insert_key(data, caps, true);
            }
            return;
        }

        if data == KEY_LEFT_BRACKET {
            self.check_for_standalone_char(data, caps, KEY_O);
            return;
        }
        if data == KEY_RIGHT_BRACKET {
            self.check_for_standalone_char(data, caps, KEY_U);
            return;
        }

        if self.is_key_d(data) {
            let mut is_correct;
            self.is_changed = false;
            for pattern in CONSONANT_D {
                if (self.index as usize) < pattern.len() {
                    continue;
                }
                is_correct = self.check_correct_vowel(pattern, data);
                // Allow "d" after a consonant.
                if !is_correct
                    && self.index >= 2
                    && self.chr(self.index - 1) == KEY_D
                    && is_consonant(self.chr(self.index - 2))
                {
                    is_correct = true;
                }
                if is_correct {
                    self.is_changed = true;
                    self.insert_d();
                    break;
                }
            }
            if !self.is_changed {
                self.insert_key(data, caps, true);
            }
            return;
        }

        if self.is_mark_key(data) {
            self.is_changed = false;
            'vowels: for &vowel_key in &VOWEL_FOR_MARK_KEYS {
                for pattern in vowel_for_mark(vowel_key) {
                    if (self.index as usize) < pattern.len() {
                        continue;
                    }
                    if self.check_correct_vowel(pattern, data) {
                        self.is_changed = true;
                        let mark = if self.processing(0) == data {
                            MARK1_MASK
                        } else if self.processing(1) == data {
                            MARK2_MASK
                        } else if self.processing(2) == data {
                            MARK3_MASK
                        } else if self.processing(3) == data {
                            MARK4_MASK
                        } else {
                            MARK5_MASK
                        };
                        self.insert_mark(mark, true);
                        break 'vowels;
                    }
                }
            }
            if !self.is_changed {
                self.insert_key(data, caps, true);
            }
            return;
        }

        // Vowel keys.
        let vni = self.is_vni();
        if vni {
            for i in (0..self.index).rev() {
                if matches!(self.chr(i), KEY_O | KEY_A | KEY_E) {
                    self.vei = i;
                    break;
                }
            }
        }

        let key_for_aeo = if !vni {
            data
        } else if data == KEY_7 || data == KEY_8 {
            KEY_W
        } else if data == KEY_6 {
            self.tw(self.vei) as u16
        } else {
            data
        };

        self.is_changed = false;
        for pattern in vowel(key_for_aeo) {
            if (self.index as usize) < pattern.len() {
                continue;
            }
            if !self.check_correct_vowel(pattern, data) {
                continue;
            }
            self.is_changed = true;
            if self.is_key_double(data) {
                self.insert_aoe(key_for_aeo);
            } else if self.is_key_w(data) {
                if vni {
                    for j in (0..self.index).rev() {
                        if matches!(self.chr(j), KEY_O | KEY_U | KEY_A | KEY_E) {
                            self.vei = j;
                            break;
                        }
                    }
                    let vei = self.vei;
                    if (data == KEY_7
                        && self.chr(vei) == KEY_A
                        && (vei < 1 || self.chr(vei - 1) != KEY_U))
                        || (data == KEY_8 && matches!(self.chr(vei), KEY_O | KEY_U))
                    {
                        break;
                    }
                }
                self.insert_w();
            }
            break;
        }

        if !self.is_changed {
            if data == KEY_W && self.config.input_type != InputType::SimpleTelex1 {
                self.check_for_standalone_char(data, caps, KEY_U);
            } else {
                self.insert_key(data, caps, true);
            }
        }
    }

    fn handle_quick_telex(&mut self, data: u16, caps: bool) -> bool {
        let Some(pair) = quick_telex(data) else {
            return false;
        };
        self.hs.code = HookCode::WillProcess;
        self.hs.backspace_count = 1;
        self.hs.new_char_count = 2;
        self.set_data(1, u32::from(pair[0]) | caps_bit(caps));
        self.set_data(0, u32::from(pair[1]) | caps_bit(caps));
        self.insert_key(pair[1], caps, false);
        true
    }

    fn check_restore_if_wrong_spelling(&mut self, code: HookCode) -> bool {
        for ii in 0..self.index {
            if !is_consonant(self.chr(ii)) && self.tw(ii) & (MARK_MASK | TONE_MASK | TONEW_MASK) != 0 {
                self.hs.code = code;
                self.hs.backspace_count = self.index;
                self.hs.new_char_count = self.state_index;
                let count = self.state_index;
                for i in 0..count {
                    let value = self.key_states[i as usize];
                    self.tw_set(i, value);
                    self.set_data(count - 1 - i, value);
                }
                self.index = count;
                return true;
            }
        }
        false
    }

    fn check_quick_consonant(&mut self) -> bool {
        if self.index <= 1 {
            return false;
        }
        let mut handled = false;

        if self.config.quick_start_consonant
            && let Some(pair) = quick_start_consonant(self.chr(0))
        {
            self.hs.code = HookCode::Restore;
            self.hs.backspace_count = self.index;
            self.hs.new_char_count = self.index + 1;
            if self.index < MAX - 1 {
                self.index += 1;
            }
            for i in (2..self.index).rev() {
                let value = self.tw(i - 1);
                self.tw_set(i, value);
            }
            let both_caps = self.tw(0) & CAPS_MASK != 0 && self.tw(2) & CAPS_MASK != 0;
            self.tw_set(1, u32::from(pair[1]) | caps_bit(both_caps));
            self.tw_set(0, u32::from(pair[0]) | (self.tw(0) & CAPS_MASK));
            handled = true;
        }

        if self.config.quick_end_consonant
            && self.index >= 2
            && !is_consonant(self.chr(self.index - 2))
            && let Some(pair) = quick_end_consonant(self.chr(self.index - 1))
        {
            self.hs.code = HookCode::Restore;
            if handled {
                self.hs.new_char_count += 1;
            } else {
                self.hs.backspace_count = 1;
                self.hs.new_char_count = 2;
            }
            if self.index < MAX - 1 {
                self.index += 1;
            }
            let caps = self.tw(self.index - 2) & CAPS_MASK;
            self.tw_set(self.index - 1, u32::from(pair[1]) | caps);
            self.tw_set(self.index - 2, u32::from(pair[0]) | caps);
            handled = true;
        }

        if handled {
            self.has_handle_quick_consonant = true;
            for i in (0..self.index).rev() {
                let value = self.get(self.tw(i));
                self.set_data(self.index - 1 - i, value);
            }
        }
        handled
    }

    fn find_macro(&mut self) -> bool {
        let found = self
            .macros
            .find(&mut self.hs.macro_key, self.config.auto_caps_macro);
        match found {
            Some(data) => {
                self.hs.macro_data = data;
                true
            }
            None => false,
        }
    }

    // ---------------------------------------------------------------------
    // Entry points
    // ---------------------------------------------------------------------

    /// Tracks typed keys in English mode so macros keep working.
    pub fn english_mode(&mut self, state: KeyState, data: KeyCode, is_caps: bool, other_control: bool) {
        self.hs.code = HookCode::DoNothing;
        if state == KeyState::MouseDown || (other_control && !is_caps) {
            self.hs.macro_key.clear();
            self.will_temp_off_engine = false;
        } else if data == KEY_SPACE {
            if !self.has_handled_macro && self.find_macro() {
                self.hs.code = HookCode::ReplaceMacro;
                self.hs.backspace_count = self.hs.macro_key.len() as i32;
            }
            self.hs.macro_key.clear();
            self.will_temp_off_engine = false;
        } else if data == KEY_DELETE {
            if self.hs.macro_key.pop().is_none() {
                self.will_temp_off_engine = false;
            }
        } else if is_word_break(KeyEvent::Keyboard, data) && !is_char_key_code(data) {
            self.hs.macro_key.clear();
            self.will_temp_off_engine = false;
        } else if !self.will_temp_off_engine {
            self.hs.macro_key.push(u32::from(data) | caps_bit(is_caps));
        }
    }

    /// Main entry point: processes one event.
    ///
    /// `caps_status` is `1` when shift is held, `2` when caps lock is on and
    /// `0` otherwise. `other_control` is `true` while Ctrl/Alt (or another
    /// command modifier) is held.
    pub fn handle_event(
        &mut self,
        event: KeyEvent,
        state: KeyState,
        data: KeyCode,
        caps_status: u8,
        other_control: bool,
    ) {
        self.is_caps = caps_status == 1 || caps_status == 2;
        let caps = self.is_caps;

        if (is_number_key(data) && caps_status == 1)
            || other_control
            || is_word_break(event, data)
            || (self.index == 0 && is_number_key(data))
        {
            self.handle_word_break(event, state, data);
        } else if data == KEY_SPACE {
            self.handle_space();
        } else if data == KEY_DELETE {
            self.handle_backspace();
        } else {
            self.handle_normal_key(data, caps);
        }
    }

    fn handle_word_break(&mut self, event: KeyEvent, state: KeyState, data: KeyCode) {
        let caps = self.is_caps;
        self.hs.code = HookCode::DoNothing;
        self.hs.backspace_count = 0;
        self.hs.new_char_count = 0;
        self.hs.ext_code = 1;

        let quick_consonant = self.config.quick_start_consonant || self.config.quick_end_consonant;
        if self.config.use_macro
            && is_macro_break_code(data)
            && !self.has_handled_macro
            && self.find_macro()
        {
            self.hs.code = HookCode::ReplaceMacro;
            self.hs.backspace_count = self.hs.macro_key.len() as i32;
            self.has_handled_macro = true;
        } else if quick_consonant && !self.temp_disable_key && is_macro_break_code(data) {
            self.check_quick_consonant();
        } else if self.config.restore_if_wrong_spelling && is_word_break(event, data) {
            if !self.temp_disable_key && self.spell_check {
                self.check_spelling(true);
            }
            if self.temp_disable_key
                && !self.check_restore_if_wrong_spelling(HookCode::RestoreAndStartNewSession)
            {
                self.hs.code = HookCode::DoNothing;
            }
        }

        let is_char_key = state == KeyState::KeyDown && is_char_key_code(data);
        if !is_char_key {
            self.special_char.clear();
            self.typing_states.clear();
        } else {
            if self.space_count > 0 {
                self.save_word_repeated(u32::from(KEY_SPACE), self.space_count);
                self.space_count = 0;
            } else {
                self.save_word();
            }
            self.special_char.push(u32::from(data) | caps_bit(caps));
            self.hs.ext_code = 3;
        }

        if self.hs.code == HookCode::DoNothing {
            self.start_new_session();
            self.spell_check = self.use_spell_checking_before;
            self.will_temp_off_engine = false;
        } else if self.hs.code == HookCode::ReplaceMacro || self.has_handle_quick_consonant {
            self.index = 0;
        }

        if self.config.use_macro {
            if is_char_key {
                self.hs.macro_key.push(u32::from(data) | caps_bit(caps));
            } else {
                self.hs.macro_key.clear();
            }
        }

        if self.config.upper_case_first_char {
            self.upper_case_status = if data == KEY_DOT {
                1
            } else if data == KEY_ENTER || data == KEY_RETURN {
                2
            } else {
                0
            };
        }
    }

    fn handle_space(&mut self) {
        if !self.temp_disable_key && self.spell_check {
            self.check_spelling(true);
        }
        let quick_consonant = self.config.quick_start_consonant || self.config.quick_end_consonant;
        if self.config.use_macro && !self.has_handled_macro && self.find_macro() {
            self.hs.code = HookCode::ReplaceMacro;
            self.hs.backspace_count = self.hs.macro_key.len() as i32;
            self.space_count += 1;
            self.has_handled_macro = true;
        } else if quick_consonant && !self.temp_disable_key && self.check_quick_consonant() {
            self.space_count += 1;
        } else if self.config.restore_if_wrong_spelling
            && self.temp_disable_key
            && !self.has_handled_macro
        {
            if !self.check_restore_if_wrong_spelling(HookCode::Restore) {
                self.hs.code = HookCode::DoNothing;
            }
            self.space_count += 1;
        } else {
            self.hs.code = HookCode::DoNothing;
            self.space_count += 1;
        }

        if self.config.use_macro {
            self.hs.macro_key.clear();
        }
        if self.config.upper_case_first_char && self.upper_case_status == 1 {
            self.upper_case_status = 2;
        }
        if self.space_count == 1 {
            if self.special_char.is_empty() {
                self.save_word();
            } else {
                self.save_special_char();
            }
        }
        self.spell_check = self.use_spell_checking_before;
        self.will_temp_off_engine = false;
    }

    fn handle_backspace(&mut self) {
        self.hs.code = HookCode::DoNothing;
        self.hs.ext_code = 2;
        if !self.special_char.is_empty() {
            self.special_char.pop();
            if self.special_char.is_empty() {
                self.restore_last_typing_state();
            }
        } else if self.space_count > 0 {
            self.space_count -= 1;
            if self.space_count == 0 {
                self.restore_last_typing_state();
            }
        } else {
            if self.state_index > 0 {
                self.state_index -= 1;
            }
            if self.index > 0 {
                self.index -= 1;
                if let Some(previous) = self.long_word_helper.pop() {
                    self.tw.copy_within(..MAX_BUFF - 1, 1);
                    self.tw[0] = previous;
                    self.index += 1;
                }
                if self.spell_check {
                    self.check_spelling(false);
                }
            }
            if self.config.use_macro {
                self.hs.macro_key.pop();
            }

            self.hs.backspace_count = 0;
            self.hs.new_char_count = 0;
            self.hs.ext_code = 2;
            if self.index == 0 {
                self.start_new_session();
                self.special_char.clear();
                self.restore_last_typing_state();
            } else {
                self.check_grammar(1);
            }
        }
    }

    fn handle_normal_key(&mut self, data: KeyCode, caps: bool) {
        if self.will_temp_off_engine {
            self.hs.code = HookCode::DoNothing;
            self.hs.ext_code = 3;
            return;
        }
        if self.space_count > 0 {
            self.hs.backspace_count = 0;
            self.hs.new_char_count = 0;
            self.hs.ext_code = 0;
            self.start_new_session();
            self.save_word_repeated(u32::from(KEY_SPACE), self.space_count);
            self.space_count = 0;
        } else if !self.special_char.is_empty() {
            self.save_special_char();
        }

        self.insert_state(data, caps);

        if !self.is_special_key(data) || self.temp_disable_key {
            if self.config.quick_telex
                && self.is_quick_telex_key(data)
                && self.handle_quick_telex(data, caps)
            {
                return;
            }
            self.hs.code = HookCode::DoNothing;
            self.hs.backspace_count = 0;
            self.hs.new_char_count = 0;
            self.hs.ext_code = 3;
            self.insert_key(data, caps, true);
        } else {
            self.hs.code = HookCode::DoNothing;
            self.hs.ext_code = 3;
            self.handle_main_key(data, caps);
        }

        if !self.config.free_mark && !self.is_key_d(data) {
            if self.hs.code == HookCode::DoNothing {
                self.check_grammar(-1);
            } else {
                self.check_grammar(0);
            }
        }

        if self.hs.code == HookCode::Restore {
            self.insert_key(data, caps, true);
            if self.state_index > 0 {
                self.state_index -= 1;
            }
        }

        if self.config.use_macro {
            match self.hs.code {
                HookCode::DoNothing => self.hs.macro_key.push(u32::from(data) | caps_bit(caps)),
                HookCode::WillProcess | HookCode::Restore => {
                    let bpc = self.hs.backspace_count;
                    for _ in 0..bpc {
                        self.hs.macro_key.pop();
                    }
                    let start = self.index - bpc;
                    for i in start..start + self.hs.new_char_count {
                        if (0..MAX).contains(&i) {
                            let value = self.tw(i);
                            self.hs.macro_key.push(value);
                        }
                    }
                }
                _ => {}
            }
        }

        if self.config.upper_case_first_char {
            if self.index == 1 && self.upper_case_status == 2 {
                self.upper_case_first_character();
            }
            self.upper_case_status = 0;
        }

        // "[" and "]" typed as plain characters.
        if is_bracket_key(data)
            && (is_bracket_key(self.hs.char_data[0] as u16)
                || matches!(
                    self.config.input_type,
                    InputType::SimpleTelex1 | InputType::SimpleTelex2
                ))
        {
            let pending = if self.hs.code == HookCode::WillProcess {
                self.hs.backspace_count
            } else {
                0
            };
            if self.index - pending > 0 {
                self.index -= 1;
                self.save_word();
            }
            self.index = 0;
            self.temp_disable_key = false;
            self.state_index = 0;
            self.hs.ext_code = 3;
            self.special_char.push(u32::from(data) | caps_bit(caps));
        }
    }
}
