//! Static Vietnamese spelling and encoding tables, transcribed from OpenKey's
//! `Vietnamese.cpp`.

use crate::keys::*;
use crate::{CONSONANT_ALLOW_MASK, END_CONSONANT_MASK, TONE_MASK, TONEW_MASK};

const EC: u16 = END_CONSONANT_MASK as u16;
const CA: u16 = CONSONANT_ALLOW_MASK as u16;
const TN: u32 = TONE_MASK;
const TW: u32 = TONEW_MASK;
const A: u32 = KEY_A as u32;
const E: u32 = KEY_E as u32;
const I: u32 = KEY_I as u32;
const O: u32 = KEY_O as u32;
const U: u32 = KEY_U as u32;
const Y: u32 = KEY_Y as u32;

/// Vowel patterns that may receive a `^`, `˘` or horn (`_vowel`).
pub(crate) fn vowel(key: u16) -> &'static [&'static [u16]] {
    match key {
        KEY_A => &[
            &[KEY_A, KEY_N, KEY_G],
            &[KEY_A, KEY_G | EC],
            &[KEY_A, KEY_N],
            &[KEY_A, KEY_M],
            &[KEY_A, KEY_U],
            &[KEY_A, KEY_Y],
            &[KEY_A, KEY_T],
            &[KEY_A, KEY_P],
            &[KEY_A],
            &[KEY_A, KEY_C],
        ],
        KEY_O => &[
            &[KEY_O, KEY_N, KEY_G],
            &[KEY_O, KEY_G | EC],
            &[KEY_O, KEY_N],
            &[KEY_O, KEY_M],
            &[KEY_O, KEY_I],
            &[KEY_O, KEY_C],
            &[KEY_O, KEY_T],
            &[KEY_O, KEY_P],
            &[KEY_O],
        ],
        KEY_E => &[
            &[KEY_E, KEY_N, KEY_H],
            &[KEY_E, KEY_H | EC],
            &[KEY_E, KEY_N, KEY_G],
            &[KEY_E, KEY_G | EC],
            &[KEY_E, KEY_C, KEY_H],
            &[KEY_E, KEY_K | EC],
            &[KEY_E, KEY_C],
            &[KEY_E, KEY_T],
            &[KEY_E, KEY_Y],
            &[KEY_E, KEY_U],
            &[KEY_E, KEY_P],
            &[KEY_E, KEY_C],
            &[KEY_E, KEY_N],
            &[KEY_E, KEY_M],
            &[KEY_E],
        ],
        KEY_W => &[
            &[KEY_O, KEY_N],
            &[KEY_U, KEY_O, KEY_N, KEY_G],
            &[KEY_U, KEY_O, KEY_G | EC],
            &[KEY_U, KEY_O, KEY_N],
            &[KEY_U, KEY_O, KEY_I],
            &[KEY_U, KEY_O, KEY_C],
            &[KEY_O, KEY_I],
            &[KEY_O, KEY_P],
            &[KEY_O, KEY_M],
            &[KEY_O, KEY_A],
            &[KEY_O, KEY_T],
            &[KEY_U, KEY_N, KEY_G],
            &[KEY_U, KEY_G | EC],
            &[KEY_A, KEY_N, KEY_G],
            &[KEY_A, KEY_G | EC],
            &[KEY_U, KEY_N],
            &[KEY_U, KEY_M],
            &[KEY_U, KEY_C],
            &[KEY_U, KEY_A],
            &[KEY_U, KEY_I],
            &[KEY_U, KEY_T],
            &[KEY_U],
            &[KEY_A, KEY_P],
            &[KEY_A, KEY_T],
            &[KEY_A, KEY_M],
            &[KEY_A, KEY_N],
            &[KEY_A],
            &[KEY_A, KEY_C],
            &[KEY_A, KEY_C, KEY_H],
            &[KEY_A, KEY_K | EC],
            &[KEY_O],
            &[KEY_U, KEY_U],
        ],
        _ => &[],
    }
}

/// Valid vowel combinations (`_vowelCombine`). The first element tells whether
/// the combination may be followed by an end consonant.
pub(crate) fn vowel_combine(key: u16) -> &'static [&'static [u32]] {
    match key {
        KEY_A => &[
            &[0, A, I],
            &[0, A, O],
            &[0, A, U],
            &[0, A | TN, U],
            &[0, A, Y],
            &[0, A | TN, Y],
        ],
        KEY_E => &[&[0, E, O], &[0, E | TN, U]],
        KEY_I => &[&[1, I, E | TN, U], &[0, I, A], &[1, I, E | TN], &[0, I, U]],
        KEY_O => &[
            &[0, O, A, I],
            &[0, O, A, O],
            &[0, O, A, Y],
            &[0, O, E, O],
            &[1, O, A],
            &[1, O, A | TW],
            &[1, O, E],
            &[0, O, I],
            &[0, O | TN, I],
            &[0, O | TW, I],
            &[1, O, O],
            &[1, O | TN, O | TN],
        ],
        KEY_U => &[
            &[0, U, Y, U],
            &[1, U, Y, E | TN],
            &[0, U, Y, A],
            &[0, U | TW, O | TW, U],
            &[0, U | TW, O | TW, I],
            &[0, U, O | TN, I],
            &[0, U, A | TN, Y],
            &[1, U, A, O],
            &[1, U, A],
            &[1, U, A | TW],
            &[1, U, A | TN],
            &[0, U | TW, A],
            &[1, U, E | TN],
            &[0, U, I],
            &[0, U | TW, I],
            &[1, U, O],
            &[1, U, O | TN],
            &[0, U, O | TW],
            &[1, U | TW, O | TW],
            &[0, U | TW, U],
            &[1, U, Y],
        ],
        KEY_Y => &[&[0, Y, E | TN, U], &[1, Y, E | TN]],
        _ => &[],
    }
}

/// Words that may receive the `đ` stroke (`_consonantD`).
pub(crate) static CONSONANT_D: &[&[u16]] = &[
    &[KEY_D, KEY_E, KEY_N, KEY_H],
    &[KEY_D, KEY_E, KEY_H | EC],
    &[KEY_D, KEY_E, KEY_N, KEY_G],
    &[KEY_D, KEY_E, KEY_G | EC],
    &[KEY_D, KEY_E, KEY_C, KEY_H],
    &[KEY_D, KEY_E, KEY_K | EC],
    &[KEY_D, KEY_E, KEY_N],
    &[KEY_D, KEY_E, KEY_C],
    &[KEY_D, KEY_E, KEY_M],
    &[KEY_D, KEY_E],
    &[KEY_D, KEY_E, KEY_T],
    &[KEY_D, KEY_E, KEY_U],
    &[KEY_D, KEY_E, KEY_O],
    &[KEY_D, KEY_E, KEY_P],
    &[KEY_D, KEY_U, KEY_N, KEY_G],
    &[KEY_D, KEY_U, KEY_G | EC],
    &[KEY_D, KEY_U, KEY_N],
    &[KEY_D, KEY_U, KEY_M],
    &[KEY_D, KEY_U, KEY_C],
    &[KEY_D, KEY_U, KEY_O],
    &[KEY_D, KEY_U, KEY_A],
    &[KEY_D, KEY_U, KEY_O, KEY_I],
    &[KEY_D, KEY_U, KEY_O, KEY_C],
    &[KEY_D, KEY_U, KEY_O, KEY_N],
    &[KEY_D, KEY_U, KEY_O, KEY_N, KEY_G],
    &[KEY_D, KEY_U, KEY_O, KEY_G | EC],
    &[KEY_D, KEY_U],
    &[KEY_D, KEY_U, KEY_P],
    &[KEY_D, KEY_U, KEY_T],
    &[KEY_D, KEY_U, KEY_I],
    &[KEY_D, KEY_I, KEY_C, KEY_H],
    &[KEY_D, KEY_I, KEY_K | EC],
    &[KEY_D, KEY_I, KEY_C],
    &[KEY_D, KEY_I, KEY_N, KEY_H],
    &[KEY_D, KEY_I, KEY_H | EC],
    &[KEY_D, KEY_I, KEY_N],
    &[KEY_D, KEY_I],
    &[KEY_D, KEY_I, KEY_A],
    &[KEY_D, KEY_I, KEY_E],
    &[KEY_D, KEY_I, KEY_E, KEY_C],
    &[KEY_D, KEY_I, KEY_E, KEY_U],
    &[KEY_D, KEY_I, KEY_E, KEY_N],
    &[KEY_D, KEY_I, KEY_E, KEY_M],
    &[KEY_D, KEY_I, KEY_E, KEY_P],
    &[KEY_D, KEY_I, KEY_T],
    &[KEY_D, KEY_O],
    &[KEY_D, KEY_O, KEY_A],
    &[KEY_D, KEY_O, KEY_A, KEY_N],
    &[KEY_D, KEY_O, KEY_A, KEY_N, KEY_G],
    &[KEY_D, KEY_O, KEY_A, KEY_G | EC],
    &[KEY_D, KEY_O, KEY_A, KEY_N, KEY_H],
    &[KEY_D, KEY_O, KEY_A, KEY_H | EC],
    &[KEY_D, KEY_O, KEY_A, KEY_M],
    &[KEY_D, KEY_O, KEY_E],
    &[KEY_D, KEY_O, KEY_I],
    &[KEY_D, KEY_O, KEY_P],
    &[KEY_D, KEY_O, KEY_C],
    &[KEY_D, KEY_O, KEY_N],
    &[KEY_D, KEY_O, KEY_N, KEY_G],
    &[KEY_D, KEY_O, KEY_G | EC],
    &[KEY_D, KEY_O, KEY_M],
    &[KEY_D, KEY_O, KEY_T],
    &[KEY_D, KEY_A],
    &[KEY_D, KEY_A, KEY_T],
    &[KEY_D, KEY_A, KEY_Y],
    &[KEY_D, KEY_A, KEY_U],
    &[KEY_D, KEY_A, KEY_I],
    &[KEY_D, KEY_A, KEY_O],
    &[KEY_D, KEY_A, KEY_P],
    &[KEY_D, KEY_A, KEY_C],
    &[KEY_D, KEY_A, KEY_C, KEY_H],
    &[KEY_D, KEY_A, KEY_K | EC],
    &[KEY_D, KEY_A, KEY_N],
    &[KEY_D, KEY_A, KEY_N, KEY_H],
    &[KEY_D, KEY_A, KEY_H | EC],
    &[KEY_D, KEY_A, KEY_N, KEY_G],
    &[KEY_D, KEY_A, KEY_G | EC],
    &[KEY_D, KEY_A, KEY_M],
    &[KEY_D],
];

/// Keys of [`vowel_for_mark`] in OpenKey's `std::map` iteration order.
pub(crate) const VOWEL_FOR_MARK_KEYS: [u16; 6] = [KEY_A, KEY_E, KEY_I, KEY_O, KEY_U, KEY_Y];

/// Patterns that may receive a tone mark (`_vowelForMark`).
pub(crate) fn vowel_for_mark(key: u16) -> &'static [&'static [u16]] {
    match key {
        KEY_A => &[
            &[KEY_A, KEY_N, KEY_G],
            &[KEY_A, KEY_G | EC],
            &[KEY_A, KEY_N],
            &[KEY_A, KEY_N, KEY_H],
            &[KEY_A, KEY_H | EC],
            &[KEY_A, KEY_M],
            &[KEY_A, KEY_U],
            &[KEY_A, KEY_Y],
            &[KEY_A, KEY_T],
            &[KEY_A, KEY_P],
            &[KEY_A],
            &[KEY_A, KEY_C],
            &[KEY_A, KEY_I],
            &[KEY_A, KEY_O],
            &[KEY_A, KEY_C, KEY_H],
            &[KEY_A, KEY_K | EC],
        ],
        KEY_O => &[
            &[KEY_O, KEY_O, KEY_N, KEY_G],
            &[KEY_O, KEY_O, KEY_G | EC],
            &[KEY_O, KEY_N, KEY_G],
            &[KEY_O, KEY_G | EC],
            &[KEY_O, KEY_O, KEY_N],
            &[KEY_O, KEY_O, KEY_C],
            &[KEY_O, KEY_O],
            &[KEY_O, KEY_N],
            &[KEY_O, KEY_M],
            &[KEY_O, KEY_I],
            &[KEY_O, KEY_C],
            &[KEY_O, KEY_T],
            &[KEY_O, KEY_P],
            &[KEY_O],
        ],
        KEY_E => &[
            &[KEY_E, KEY_N, KEY_H],
            &[KEY_E, KEY_H | EC],
            &[KEY_E, KEY_N, KEY_G],
            &[KEY_E, KEY_G | EC],
            &[KEY_E, KEY_C, KEY_H],
            &[KEY_E, KEY_K | EC],
            &[KEY_E, KEY_C],
            &[KEY_E, KEY_T],
            &[KEY_E, KEY_Y],
            &[KEY_E, KEY_U],
            &[KEY_E, KEY_P],
            &[KEY_E, KEY_C],
            &[KEY_E, KEY_N],
            &[KEY_E, KEY_M],
            &[KEY_E],
        ],
        KEY_I => &[
            &[KEY_I, KEY_N, KEY_H],
            &[KEY_I, KEY_H | EC],
            &[KEY_I, KEY_C, KEY_H],
            &[KEY_I, KEY_K | EC],
            &[KEY_I, KEY_N],
            &[KEY_I, KEY_T],
            &[KEY_I, KEY_U],
            &[KEY_I, KEY_U, KEY_P],
            &[KEY_I, KEY_N],
            &[KEY_I, KEY_M],
            &[KEY_I, KEY_P],
            &[KEY_I, KEY_A],
            &[KEY_I, KEY_C],
            &[KEY_I],
        ],
        KEY_U => &[
            &[KEY_U, KEY_N, KEY_G],
            &[KEY_U, KEY_G | EC],
            &[KEY_U, KEY_I],
            &[KEY_U, KEY_O],
            &[KEY_U, KEY_Y],
            &[KEY_U, KEY_Y, KEY_N],
            &[KEY_U, KEY_Y, KEY_T],
            &[KEY_U, KEY_Y, KEY_P],
            &[KEY_U, KEY_Y, KEY_C, KEY_H],
            &[KEY_U, KEY_Y, KEY_K | EC],
            &[KEY_U, KEY_Y, KEY_N, KEY_H],
            &[KEY_U, KEY_Y, KEY_H | EC],
            &[KEY_U, KEY_T],
            &[KEY_U, KEY_U],
            &[KEY_U, KEY_A],
            &[KEY_U, KEY_I],
            &[KEY_U, KEY_C],
            &[KEY_U, KEY_N],
            &[KEY_U, KEY_M],
            &[KEY_U, KEY_P],
            &[KEY_U],
        ],
        KEY_Y => &[&[KEY_Y]],
        _ => &[],
    }
}

/// Valid initial consonants (`_consonantTable`).
pub(crate) static CONSONANT_TABLE: &[&[u16]] = &[
    &[KEY_N, KEY_G, KEY_H],
    &[KEY_P, KEY_H],
    &[KEY_T, KEY_H],
    &[KEY_T, KEY_R],
    &[KEY_G, KEY_I],
    &[KEY_C, KEY_H],
    &[KEY_N, KEY_H],
    &[KEY_N, KEY_G],
    &[KEY_K, KEY_H],
    &[KEY_G, KEY_H],
    &[KEY_G],
    &[KEY_C],
    &[KEY_Q],
    &[KEY_K],
    &[KEY_T],
    &[KEY_R],
    &[KEY_H],
    &[KEY_B],
    &[KEY_M],
    &[KEY_V],
    &[KEY_N],
    &[KEY_L],
    &[KEY_X],
    &[KEY_P],
    &[KEY_S],
    &[KEY_D],
    &[KEY_F | CA],
    &[KEY_W | CA],
    &[KEY_Z | CA],
    &[KEY_J | CA],
    &[KEY_F | EC],
    &[KEY_W | EC],
    &[KEY_J | EC],
];

/// Valid end consonants (`_endConsonantTable`).
pub(crate) static END_CONSONANT_TABLE: &[&[u16]] = &[
    &[KEY_T],
    &[KEY_P],
    &[KEY_C],
    &[KEY_N],
    &[KEY_M],
    &[KEY_G | EC],
    &[KEY_K | EC],
    &[KEY_H | EC],
    &[KEY_C, KEY_H],
    &[KEY_N, KEY_H],
    &[KEY_N, KEY_G],
];

/// Keys that cannot become a standalone `ư` (`_standaloneWbad`).
pub(crate) static STANDALONE_W_BAD: &[u16] = &[KEY_W, KEY_E, KEY_Y, KEY_F, KEY_J, KEY_K, KEY_Z];

/// Consonant pairs allowed before a standalone `ư` (`_doubleWAllowed`).
pub(crate) static DOUBLE_W_ALLOWED: &[[u16; 2]] = &[
    [KEY_T, KEY_R],
    [KEY_T, KEY_H],
    [KEY_C, KEY_H],
    [KEY_N, KEY_H],
    [KEY_N, KEY_G],
    [KEY_K, KEY_H],
    [KEY_G, KEY_I],
    [KEY_P, KEY_H],
    [KEY_G, KEY_H],
];

/// Quick start consonant (`f` → `ph`, `j` → `gi`, `w` → `qu`).
pub(crate) fn quick_start_consonant(key: u16) -> Option<[u16; 2]> {
    match key {
        KEY_F => Some([KEY_P, KEY_H]),
        KEY_J => Some([KEY_G, KEY_I]),
        KEY_W => Some([KEY_Q, KEY_U]),
        _ => None,
    }
}

/// Quick end consonant (`g` → `ng`, `h` → `nh`, `k` → `ch`).
pub(crate) fn quick_end_consonant(key: u16) -> Option<[u16; 2]> {
    match key {
        KEY_G => Some([KEY_N, KEY_G]),
        KEY_H => Some([KEY_N, KEY_H]),
        KEY_K => Some([KEY_C, KEY_H]),
        _ => None,
    }
}

/// Quick Telex (`cc` → `ch`, `gg` → `gi`, ...).
pub(crate) fn quick_telex(key: u16) -> Option<[u16; 2]> {
    match key {
        KEY_C => Some([KEY_C, KEY_H]),
        KEY_G => Some([KEY_G, KEY_I]),
        KEY_K => Some([KEY_K, KEY_H]),
        KEY_N => Some([KEY_N, KEY_G]),
        KEY_Q => Some([KEY_Q, KEY_U]),
        KEY_P => Some([KEY_P, KEY_H]),
        KEY_T => Some([KEY_T, KEY_H]),
        KEY_U => Some([KEY_U, KEY_U]),
        _ => None,
    }
}

/// Combining marks (sắc, huyền, hỏi, ngã, nặng) for Unicode compound output.
pub(crate) const UNICODE_COMPOUND_MARK: [u16; 5] = [0x0301, 0x0300, 0x0309, 0x0303, 0x0323];
