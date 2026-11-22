//! Host independent glue between key events and the [`Engine`].
//!
//! The [`Processor`] reproduces the logic of OpenKey's keyboard hook: it
//! tracks modifiers and hotkeys, feeds the engine, and converts the engine's
//! [`HookState`](crate::HookState) into a flat list of [`OutputOp`]s that a
//! platform layer simply has to replay.

use crate::code_table::CodeTable;
use crate::config::Config;
use crate::engine::{Engine, HookCode, KeyEvent, KeyState, MAX_BUFF};
use crate::hotkey::{Disposition, HotkeyAction, HotkeyDetector, Modifiers};
use crate::keys::*;
use crate::macros::MacroTable;
use crate::vietnamese::UNICODE_COMPOUND_MARK;
use crate::{CAPS_MASK, CHAR_CODE_MASK, PURE_CHARACTER_MASK};

/// Invisible character sent before replacements when
/// [`Config::fix_recommend_browser`] is enabled.
const EMPTY_CHARACTER: char = '\u{202F}';

/// A key event delivered by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    /// Platform neutral key code (see [`crate::keys`]).
    pub key: KeyCode,
    /// `true` for key press, `false` for release.
    pub down: bool,
    /// Whether Caps Lock is on.
    pub caps_lock: bool,
}

impl KeyInput {
    /// A key press without Caps Lock.
    pub fn down(key: KeyCode) -> Self {
        Self {
            key,
            down: true,
            caps_lock: false,
        }
    }

    /// A key release without Caps Lock.
    pub fn up(key: KeyCode) -> Self {
        Self {
            key,
            down: false,
            caps_lock: false,
        }
    }

    /// Sets the Caps Lock state.
    pub fn with_caps_lock(mut self, caps_lock: bool) -> Self {
        self.caps_lock = caps_lock;
        self
    }
}

/// A primitive action the host must perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutputOp {
    /// Press backspace once.
    Backspace,
    /// Type a Unicode character.
    Char(char),
    /// Press a key that has no character (e.g. Enter).
    Key(KeyCode),
}

/// Result of processing one key event.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    /// `true` if the original key must be blocked. When `false` the host must
    /// let the original key through **after** replaying `ops`.
    pub consumed: bool,
    /// Actions to replay, in order.
    pub ops: Vec<OutputOp>,
    /// `Some(vietnamese)` when the language was switched by the hotkey.
    pub language_changed: Option<bool>,
    /// The host should beep (language switched and beeping enabled).
    pub beep: bool,
}

impl Output {
    fn pass_through() -> Self {
        Self::default()
    }

    fn consumed() -> Self {
        Self {
            consumed: true,
            ..Self::default()
        }
    }

    /// Simulates the effect of this output on a plain text buffer. Mainly
    /// useful for tests and examples.
    ///
    /// `key` and `shift` describe the original key, which is applied when the
    /// output is not consumed.
    pub fn apply_to(&self, text: &mut String, key: KeyCode, shift: bool) {
        for op in &self.ops {
            match *op {
                OutputOp::Backspace => {
                    text.pop();
                }
                OutputOp::Char(c) => text.push(c),
                OutputOp::Key(k) => apply_key(text, k, false),
            }
        }
        if !self.consumed {
            apply_key(text, key, shift);
        }
    }
}

fn apply_key(text: &mut String, key: KeyCode, shift: bool) {
    if key == KEY_DELETE {
        text.pop();
        return;
    }
    let code = u32::from(key) | if shift { CAPS_MASK } else { 0 };
    if let Some(c) = key_code_to_char(code) {
        text.push(c);
    } else if key == KEY_ENTER {
        text.push('\n');
    } else if key == KEY_TAB {
        text.push('\t');
    }
}

/// Keys that never influence the current word (lock keys).
fn is_ignored_key(key: KeyCode) -> bool {
    matches!(key, 0x14 | KEY_NUM_LOCK | KEY_SCROLL_LOCK)
}

/// Keys understood by the engine. Every other key (Home, F1, ...) ends the word.
fn is_engine_key(key: KeyCode) -> bool {
    is_letter(key)
        || is_number(key)
        || key_code_to_char(u32::from(key)).is_some()
        || matches!(
            key,
            KEY_DELETE | KEY_ENTER | KEY_TAB | KEY_ESC | KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN
        )
}

/// Collects output operations while keeping track of how many output units
/// each visible character occupies (needed by multi-unit code tables).
#[derive(Debug, Clone, Default)]
struct Sink {
    ops: Vec<OutputOp>,
    sync: Vec<u8>,
}

impl Sink {
    fn backspace(&mut self, table: CodeTable) {
        self.ops.push(OutputOp::Backspace);
        if table.is_multi_unit() && self.sync.pop().is_some_and(|len| len > 1) {
            self.ops.push(OutputOp::Backspace);
        }
    }

    fn track(&mut self, table: CodeTable, len: u8) {
        if table.is_multi_unit() {
            self.sync.push(len);
            if self.sync.len() > MAX_BUFF * 4 {
                self.sync.drain(..MAX_BUFF);
            }
        }
    }

    fn push_unit(&mut self, unit: u16) {
        if let Some(c) = char::from_u32(u32::from(unit)) {
            self.ops.push(OutputOp::Char(c));
        }
    }

    fn empty_char(&mut self, table: CodeTable) {
        self.track(table, 1);
        self.ops.push(OutputOp::Char(EMPTY_CHARACTER));
    }

    fn pure_char(&mut self, table: CodeTable, data: u32) {
        self.track(table, 1);
        if let Some(c) = char::from_u32(data & !PURE_CHARACTER_MASK) {
            self.ops.push(OutputOp::Char(c));
        }
    }

    fn key_code(&mut self, table: CodeTable, data: u32) {
        if data & CHAR_CODE_MASK == 0 {
            self.track(table, 1);
            match key_code_to_char(data) {
                Some(c) => self.ops.push(OutputOp::Char(c)),
                None => self.ops.push(OutputOp::Key(data as KeyCode)),
            }
            return;
        }
        let code = data as u16;
        match table {
            CodeTable::Unicode => self.push_unit(code),
            CodeTable::Tcvn3 | CodeTable::VniWindows | CodeTable::Cp1258 => {
                let [low, high] = code.to_le_bytes();
                self.push_unit(u16::from(low));
                if high > 32 {
                    self.track(table, 2);
                    self.push_unit(u16::from(high));
                } else {
                    self.track(table, 1);
                }
            }
            CodeTable::UnicodeCompound => {
                let mark = usize::from(code >> 13);
                self.track(table, if mark > 0 { 2 } else { 1 });
                self.push_unit(code & 0x1FFF);
                if let Some(&combining) = mark.checked_sub(1).and_then(|i| UNICODE_COMPOUND_MARK.get(i))
                {
                    self.push_unit(combining);
                }
            }
        }
    }
}

/// Complete, platform independent input method logic.
#[derive(Debug, Clone)]
pub struct Processor {
    engine: Engine,
    hotkeys: HotkeyDetector,
    sink: Sink,
    swallowed_key: Option<KeyCode>,
}

impl Default for Processor {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Processor {
    /// Creates a processor with the given configuration.
    pub fn new(config: Config) -> Self {
        Self {
            engine: Engine::new(config),
            hotkeys: HotkeyDetector::default(),
            sink: Sink::default(),
            swallowed_key: None,
        }
    }

    /// Current configuration.
    pub fn config(&self) -> &Config {
        self.engine.config()
    }

    /// Replaces the configuration and starts a new word.
    pub fn set_config(&mut self, config: Config) {
        self.engine.set_config(config);
        self.reset();
    }

    /// Switches between Vietnamese (`true`) and English (`false`).
    pub fn set_vietnamese(&mut self, vietnamese: bool) {
        if self.engine.config().vietnamese != vietnamese {
            let mut config = self.engine.config().clone();
            config.vietnamese = vietnamese;
            self.engine.set_config(config);
        }
        self.reset();
    }

    /// Macro table.
    pub fn macros(&self) -> &MacroTable {
        self.engine.macros()
    }

    /// Replaces the macro table.
    pub fn set_macros(&mut self, macros: MacroTable) {
        self.engine.set_macros(macros);
    }

    /// Underlying engine.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Mutable access to the underlying engine.
    pub fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    /// Currently held modifiers.
    pub fn modifiers(&self) -> Modifiers {
        self.hotkeys.modifiers()
    }

    /// Forgets the current word (e.g. on focus change).
    pub fn reset(&mut self) {
        self.engine.start_new_session();
        self.sink.sync.clear();
    }

    /// Forgets the current word and the modifier state (e.g. after the
    /// workstation was locked and key-up events may have been lost).
    pub fn reset_all(&mut self) {
        self.reset();
        self.hotkeys.reset();
        self.swallowed_key = None;
    }

    /// Notifies the processor about a mouse click (the caret may have moved).
    pub fn handle_mouse_click(&mut self) {
        if self.engine.config().vietnamese {
            self.engine
                .handle_event(KeyEvent::Mouse, KeyState::MouseDown, 0, 0, false);
        } else {
            self.engine.english_mode(KeyState::MouseDown, 0, false, false);
        }
        self.sink.sync.clear();
    }

    /// Convenience wrapper for [`Processor::handle_key`] with a key press.
    pub fn handle_key_down(&mut self, input: KeyInput) -> Output {
        self.handle_key(KeyInput { down: true, ..input })
    }

    /// Processes one keyboard event.
    pub fn handle_key(&mut self, input: KeyInput) -> Output {
        let key = input.key;
        if !input.down && self.swallowed_key == Some(key) {
            self.swallowed_key = None;
            return Output::consumed();
        }

        let config = self.engine.config();
        let (disposition, actions) = self.hotkeys.handle(
            key,
            input.down,
            &config.switch_hotkey,
            config.temp_off_spelling,
            config.temp_off_engine,
        );
        let mut output = Output::pass_through();
        for action in actions {
            self.apply_action(action, &mut output);
        }
        match disposition {
            Disposition::Swallow => {
                self.swallowed_key = Some(key);
                output.consumed = true;
                return output;
            }
            Disposition::PassThrough => return output,
            Disposition::Continue => {}
        }
        if !input.down || is_ignored_key(key) {
            return output;
        }
        if !is_engine_key(key) {
            self.reset();
            return output;
        }

        let modifiers = self.hotkeys.modifiers();
        let shift = modifiers.contains(Modifiers::SHIFT);
        let other_control = modifiers.contains(Modifiers::CONTROL)
            || modifiers.contains(Modifiers::ALT)
            || modifiers.contains(Modifiers::META);
        let caps_status: u8 = if is_letter(key) {
            match (shift, input.caps_lock) {
                (true, true) | (false, false) => 0,
                (true, false) => 1,
                (false, true) => 2,
            }
        } else if shift {
            1
        } else {
            0
        };
        let typed = u32::from(key) | if caps_status != 0 { CAPS_MASK } else { 0 };

        let mut result = if self.engine.config().vietnamese {
            self.engine
                .handle_event(KeyEvent::Keyboard, KeyState::KeyDown, key, caps_status, other_control);
            self.emit(typed)
        } else {
            self.handle_english(key, caps_status != 0, other_control, typed)
        };
        result.language_changed = output.language_changed;
        result.beep = output.beep;
        result
    }

    fn apply_action(&mut self, action: HotkeyAction, output: &mut Output) {
        match action {
            HotkeyAction::SwitchLanguage => {
                let vietnamese = !self.engine.config().vietnamese;
                output.beep = self.engine.config().switch_hotkey.beep;
                self.set_vietnamese(vietnamese);
                output.language_changed = Some(vietnamese);
            }
            HotkeyAction::TempOffSpelling => self.engine.temp_off_spell_checking(),
            HotkeyAction::TempOffEngine => self.engine.temp_off_engine(true),
        }
    }

    fn handle_english(&mut self, key: KeyCode, caps: bool, other_control: bool, typed: u32) -> Output {
        let config = self.engine.config();
        if !(config.use_macro && config.use_macro_in_english_mode) {
            return Output::pass_through();
        }
        self.engine
            .english_mode(KeyState::KeyDown, key, caps, other_control);
        if self.engine.state().code == HookCode::ReplaceMacro {
            self.emit(typed)
        } else {
            Output::pass_through()
        }
    }

    /// Converts the engine state into output operations.
    fn emit(&mut self, typed: u32) -> Output {
        let table = self.engine.config().code_table;
        let fix_browser = self.engine.config().fix_recommend_browser;
        let state = self.engine.state();
        let code = state.code;
        let ext_code = state.ext_code;
        let mut backspaces = state.backspace_count;
        self.sink.ops.clear();

        match code {
            HookCode::DoNothing | HookCode::BreakWord => {
                if table.is_multi_unit() {
                    match ext_code {
                        1 => self.sink.sync.clear(),
                        2 => {
                            if self.sink.sync.pop().is_some_and(|len| len > 1) {
                                self.sink.ops.push(OutputOp::Backspace);
                            }
                        }
                        3 => self.sink.track(table, 1),
                        _ => {}
                    }
                }
                return Output {
                    ops: std::mem::take(&mut self.sink.ops),
                    ..Output::pass_through()
                };
            }
            HookCode::WillProcess | HookCode::Restore | HookCode::RestoreAndStartNewSession => {
                let new_chars: Vec<u32> = state
                    .char_data
                    .iter()
                    .take(usize::try_from(state.new_char_count).unwrap_or(0))
                    .rev()
                    .copied()
                    .collect();
                if fix_browser && ext_code != 4 {
                    self.sink.empty_char(table);
                    backspaces += 1;
                }
                self.backspaces(table, backspaces);
                for data in new_chars {
                    self.sink.key_code(table, data);
                }
                if code != HookCode::WillProcess {
                    self.sink.key_code(table, typed);
                }
                if code == HookCode::RestoreAndStartNewSession {
                    self.engine.start_new_session();
                }
            }
            HookCode::ReplaceMacro => {
                let data = state.macro_data.clone();
                if fix_browser {
                    self.sink.empty_char(table);
                    backspaces += 1;
                }
                self.backspaces(table, backspaces);
                for value in data {
                    if value & PURE_CHARACTER_MASK != 0 {
                        self.sink.pure_char(table, value);
                    } else {
                        self.sink.key_code(table, value);
                    }
                }
                self.sink.key_code(table, typed);
            }
        }
        Output {
            consumed: true,
            ops: std::mem::take(&mut self.sink.ops),
            language_changed: None,
            beep: false,
        }
    }

    fn backspaces(&mut self, table: CodeTable, count: i32) {
        if count > 0 && count < MAX_BUFF as i32 {
            for _ in 0..count {
                self.sink.backspace(table);
            }
        }
    }
}
