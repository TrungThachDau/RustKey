//! # rustkey-engine
//!
//! A platform independent Vietnamese input method engine supporting Telex,
//! VNI and Simple Telex, with spell checking, macros and several output
//! encodings. The typing algorithm is a Rust port of the
//! [OpenKey](https://github.com/tuyenvm/OpenKey) engine.
//!
//! The crate has two layers:
//!
//! * [`Engine`] is the raw state machine. It consumes key events and produces
//!   a [`HookState`] describing how many characters must be deleted and which
//!   characters must be sent.
//! * [`Processor`] wraps the engine and turns its result into a list of
//!   simple [`OutputOp`]s (backspace, character, key) that any platform can
//!   replay, including the bookkeeping required for multi-unit encodings.
//!
//! Key codes are platform neutral constants from [`keys`] (their values match
//! Windows virtual-key codes).
//!
//! ```
//! use rustkey_engine::{Config, Processor, KeyInput, keys};
//!
//! let mut processor = Processor::new(Config::default());
//! let mut text = String::new();
//! for key in [keys::KEY_V, keys::KEY_I, keys::KEY_E, keys::KEY_E, keys::KEY_T, keys::KEY_J] {
//!     let output = processor.handle_key(KeyInput::down(key));
//!     output.apply_to(&mut text, key, false);
//! }
//! assert_eq!(text, "việt");
//! ```

mod code_table;
mod config;
mod engine;
mod hotkey;
pub mod keys;
mod macros;
mod processor;
mod vietnamese;

pub use code_table::CodeTable;
pub use config::{Config, InputType};
pub use engine::{Engine, HookCode, HookState, KeyEvent, KeyState, MAX_BUFF};
pub use hotkey::{Disposition, Hotkey, HotkeyAction, HotkeyDetector, Modifiers};
pub use macros::{MACRO_FILE_HEADER, MacroEntry, MacroError, MacroTable};
pub use processor::{KeyInput, Output, OutputOp, Processor};

/// Mask selecting the key code or character code of an engine value.
pub const CHAR_MASK: u32 = 0xFFFF;
/// The character is upper case.
pub const CAPS_MASK: u32 = 0x10000;
/// The character has a circumflex (`â`, `ê`, `ô`) or stroke (`đ`).
pub const TONE_MASK: u32 = 0x20000;
/// The character has a breve or horn (`ă`, `ơ`, `ư`).
pub const TONEW_MASK: u32 = 0x40000;
/// Dấu sắc.
pub const MARK1_MASK: u32 = 0x80000;
/// Dấu huyền.
pub const MARK2_MASK: u32 = 0x100000;
/// Dấu hỏi.
pub const MARK3_MASK: u32 = 0x200000;
/// Dấu ngã.
pub const MARK4_MASK: u32 = 0x400000;
/// Dấu nặng.
pub const MARK5_MASK: u32 = 0x800000;
/// Any tone mark.
pub const MARK_MASK: u32 = 0xF80000;
/// The character was produced by a standalone key (`w`, `[`, `]`).
pub const STANDALONE_MASK: u32 = 0x1000000;
/// The value is an encoded character rather than a key code.
pub const CHAR_CODE_MASK: u32 = 0x2000000;
/// The value is a raw Unicode scalar value (used by macros).
pub const PURE_CHARACTER_MASK: u32 = 0x8000_0000;

pub(crate) const END_CONSONANT_MASK: u32 = 0x4000;
pub(crate) const CONSONANT_ALLOW_MASK: u32 = 0x8000;
