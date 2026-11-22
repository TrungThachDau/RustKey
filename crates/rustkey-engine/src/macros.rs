//! Text expansion macros ("gõ tắt"), port of OpenKey's `Macro.cpp`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::code_table::{self, CodeTable, TABLE_KEYS};
use crate::engine::character_code;
use crate::keys::{char_to_key_code, key_code_to_char};
use crate::{CAPS_MASK, CHAR_CODE_MASK, PURE_CHARACTER_MASK};

/// First line of OpenKey/UniKey compatible macro files.
pub const MACRO_FILE_HEADER: &str =
    ";Compatible OpenKey Macro Data file for UniKey*** version=1 ***";

/// Errors produced by [`MacroTable`] operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MacroError {
    /// The abbreviation is empty.
    #[error("macro name must not be empty")]
    EmptyName,
}

/// A single macro definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroEntry {
    /// Abbreviation as typed by the user.
    pub text: String,
    /// Expanded text.
    pub content: String,
    content_code: Vec<u32>,
}

/// Collection of macros keyed by their encoded abbreviation.
#[derive(Debug, Clone, Default)]
pub struct MacroTable {
    table: CodeTable,
    map: BTreeMap<Vec<u32>, MacroEntry>,
}

/// Converts text into engine values for the given code table.
fn convert(text: &str, table: CodeTable) -> Vec<u32> {
    text.chars()
        .map(|c| {
            if let Some(code) = char_to_key_code(c) {
                return code;
            }
            if let Ok(unit) = u16::try_from(u32::from(c))
                && unit != 0
            {
                for &key in &TABLE_KEYS {
                    let Some(unicode_row) = code_table::lookup(CodeTable::Unicode, key) else {
                        continue;
                    };
                    if let Some(position) = unicode_row.iter().position(|&v| v == unit)
                        && let Some(&encoded) =
                            code_table::lookup(table, key).and_then(|row| row.get(position))
                    {
                        return u32::from(encoded) | CHAR_CODE_MASK;
                    }
                }
            }
            u32::from(c) | PURE_CHARACTER_MASK
        })
        .collect()
}

/// Changes the case of an engine value. Returns `true` if the value changed.
fn modify_case(code: &mut u32, upper: bool, table: CodeTable) -> bool {
    let original = *code;
    if *code & PURE_CHARACTER_MASK != 0 {
        return false;
    }
    if *code & CHAR_CODE_MASK == 0 {
        if upper {
            *code |= CAPS_MASK;
        } else {
            *code &= !CAPS_MASK;
        }
        return *code != original;
    }
    let unit = *code as u16;
    for &key in &TABLE_KEYS {
        let Some(row) = code_table::lookup(table, key) else {
            continue;
        };
        if let Some(mut position) = row.iter().position(|&v| v == unit) {
            if position % 2 == 0 && !upper {
                position += 1;
            } else if position % 2 != 0 && upper {
                position -= 1;
            }
            if let Some(&value) = row.get(position) {
                *code = u32::from(value) | CHAR_CODE_MASK;
            }
            return *code != original;
        }
    }
    false
}

impl MacroTable {
    /// Creates an empty table producing output for `table`.
    pub fn new(table: CodeTable) -> Self {
        Self {
            table,
            map: BTreeMap::new(),
        }
    }

    /// Code table used for encoded macro data.
    pub fn code_table(&self) -> CodeTable {
        self.table
    }

    /// Re-encodes all macros for a new code table.
    pub fn set_code_table(&mut self, table: CodeTable) {
        if table == self.table {
            return;
        }
        self.table = table;
        let entries: Vec<MacroEntry> = std::mem::take(&mut self.map).into_values().collect();
        for entry in entries {
            self.insert_entry(entry.text, entry.content);
        }
    }

    fn insert_entry(&mut self, text: String, content: String) {
        let key = convert(&text, self.table);
        let content_code = convert(&content, self.table);
        self.map.insert(
            key,
            MacroEntry {
                text,
                content,
                content_code,
            },
        );
    }

    /// Number of macros.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Iterates over all macros (ordered by encoded abbreviation).
    pub fn iter(&self) -> impl Iterator<Item = &MacroEntry> {
        self.map.values()
    }

    /// Returns `true` if a macro with this abbreviation exists.
    pub fn contains(&self, text: &str) -> bool {
        self.map.contains_key(&convert(text, self.table))
    }

    /// Adds a macro or replaces the content of an existing one.
    pub fn add(&mut self, text: &str, content: &str) -> Result<(), MacroError> {
        if text.is_empty() {
            return Err(MacroError::EmptyName);
        }
        let key = convert(text, self.table);
        match self.map.get_mut(&key) {
            Some(entry) => {
                entry.content = content.to_owned();
                entry.content_code = convert(content, self.table);
            }
            None => self.insert_entry(text.to_owned(), content.to_owned()),
        }
        Ok(())
    }

    /// Removes a macro. Returns `true` if it existed.
    pub fn remove(&mut self, text: &str) -> bool {
        self.map.remove(&convert(text, self.table)).is_some()
    }

    /// Removes all macros.
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Looks up the typed keys. `key` is converted in place (as OpenKey does).
    pub(crate) fn find(&self, key: &mut [u32], auto_caps: bool) -> Option<Vec<u32>> {
        for value in key.iter_mut() {
            *value = character_code(*value, self.table);
        }
        if let Some(entry) = self.map.get(key) {
            return Some(entry.content_code.clone());
        }
        if !auto_caps {
            return None;
        }

        let mut all_upper = false;
        if key.len() > 1 && modify_case(&mut key[1], false, self.table) {
            all_upper = true;
            for value in key.iter_mut().skip(2) {
                modify_case(value, false, self.table);
            }
        }
        let first_changed = key
            .first_mut()
            .is_some_and(|first| modify_case(first, false, self.table));
        if !first_changed {
            return None;
        }
        let entry = self.map.get(key)?;
        let mut data = entry.content_code.clone();
        for (index, value) in data.iter_mut().enumerate() {
            if index != 0 && !all_upper {
                break;
            }
            if let Some(c) = key_code_to_char(*value) {
                if let Some(upper) = char_to_key_code(c.to_ascii_uppercase()) {
                    *value = upper;
                }
                continue;
            }
            if *value & CHAR_CODE_MASK != 0 {
                modify_case(value, true, self.table);
            }
        }
        Some(data)
    }

    /// Parses an OpenKey/UniKey macro file (`name:content` per line after a
    /// header line). Existing macros with the same name are kept.
    pub fn load_from_str(&mut self, data: &str) {
        for line in data.lines().skip(1) {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let Some((name, content)) = line.split_once(':') else {
                continue;
            };
            let mut name = name.to_owned();
            let mut content = content.to_owned();
            while name.is_empty() && !content.is_empty() {
                let Some((head, tail)) = content.split_once(':') else {
                    break;
                };
                name.push(':');
                name.push_str(head);
                content = tail.to_owned();
            }
            if !name.is_empty() && !self.contains(&name) {
                // Name is non-empty, so adding cannot fail.
                let _ = self.add(&name, &content);
            }
        }
    }

    /// Serializes the macros in OpenKey/UniKey file format.
    pub fn to_file_string(&self) -> String {
        let mut out = String::with_capacity(64 + self.map.len() * 32);
        out.push_str(MACRO_FILE_HEADER);
        out.push('\n');
        for entry in self.map.values() {
            let _ = writeln!(out, "{}:{}", entry.text, entry.content);
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn file_round_trip() {
        let mut table = MacroTable::new(CodeTable::Unicode);
        table.add("vn", "Việt Nam").unwrap();
        table.add("ko", "không").unwrap();
        let text = table.to_file_string();
        let mut loaded = MacroTable::new(CodeTable::Unicode);
        loaded.load_from_str(&text);
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains("vn"));
        assert!(loaded.iter().any(|e| e.content == "Việt Nam"));
    }

    #[test]
    fn name_starting_with_colon() {
        let mut table = MacroTable::new(CodeTable::Unicode);
        table.load_from_str("header\r\n:):smile\r\n");
        assert!(table.contains(":)"));
    }

    #[test]
    fn empty_name_is_rejected() {
        let mut table = MacroTable::new(CodeTable::Unicode);
        assert_eq!(table.add("", "x"), Err(MacroError::EmptyName));
    }

    #[test]
    fn vietnamese_characters_are_encoded() {
        let code = convert("ệ", CodeTable::Unicode);
        assert_eq!(code, vec![0x1EC7 | CHAR_CODE_MASK]);
        let pure = convert("😀", CodeTable::Unicode);
        assert_eq!(pure, vec![0x1F600 | PURE_CHARACTER_MASK]);
    }
}
