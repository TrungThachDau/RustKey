//! Replays engine output with `SendInput`.

use rustkey_engine::OutputOp;
use rustkey_engine::keys::KeyCode;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VK_BACK,
    VK_DELETE, VK_DOWN, VK_END, VK_HOME, VK_INSERT, VK_LEFT, VK_NEXT, VK_PACKET, VK_PRIOR,
    VK_RCONTROL, VK_RIGHT, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{LLKHF_EXTENDED, LLKHF_UP};

/// `dwExtraInfo` of events injected by RustKey, so the hook can ignore them.
pub const INJECTED_MARKER: usize = 0x5255_5354;

fn keyboard_input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: INJECTED_MARKER,
            },
        },
    }
}

fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        VK_LEFT | VK_RIGHT | VK_UP | VK_DOWN | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_INSERT | VK_DELETE
            | VK_RCONTROL
    )
}

/// A single key press or release of `vk`, with its scan code so that remote
/// desktop clients (which forward scan codes) see it too.
pub fn key_event(vk: u16, up: bool) -> INPUT {
    // SAFETY: MapVirtualKeyW has no memory safety requirements.
    let scan = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC) } as u16;
    let mut flags = if is_extended(vk) { KEYEVENTF_EXTENDEDKEY } else { 0 };
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    keyboard_input(vk, scan, flags)
}

fn push_key(inputs: &mut Vec<INPUT>, vk: KeyCode) {
    inputs.push(key_event(vk, false));
    inputs.push(key_event(vk, true));
}

fn push_char(inputs: &mut Vec<INPUT>, ch: char) {
    let mut buffer = [0u16; 2];
    for &unit in ch.encode_utf16(&mut buffer).iter() {
        inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE));
        inputs.push(keyboard_input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
}

/// Converts output operations into keyboard input events.
pub fn build_inputs(ops: &[OutputOp]) -> Vec<INPUT> {
    let mut inputs = Vec::with_capacity(ops.len() * 2);
    for op in ops {
        match *op {
            OutputOp::Backspace => push_key(&mut inputs, VK_BACK),
            OutputOp::Key(key) => push_key(&mut inputs, key),
            OutputOp::Char(ch) => push_char(&mut inputs, ch),
        }
    }
    inputs
}

/// Re-creates a keyboard event seen by the low level hook (`vk`, `scan` and
/// `flags` of its `KBDLLHOOKSTRUCT`), marked as injected by RustKey.
pub fn replay_input(vk: u32, scan: u32, flags: u32) -> INPUT {
    let mut event_flags = 0;
    if flags & LLKHF_EXTENDED != 0 {
        event_flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if flags & LLKHF_UP != 0 {
        event_flags |= KEYEVENTF_KEYUP;
    }
    let vk = u16::try_from(vk).unwrap_or_default();
    let scan = u16::try_from(scan).unwrap_or_default();
    if vk == VK_PACKET {
        // Unicode input injected by another program: `scan` is the UTF-16 unit.
        return keyboard_input(0, scan, (event_flags & KEYEVENTF_KEYUP) | KEYEVENTF_UNICODE);
    }
    keyboard_input(vk, scan, event_flags)
}

/// Sends raw input events.
pub fn send_inputs(inputs: &[INPUT]) {
    if inputs.is_empty() {
        return;
    }
    let count = u32::try_from(inputs.len()).unwrap_or(u32::MAX);
    // SAFETY: `inputs` holds `count` initialized INPUT structures.
    unsafe { SendInput(count, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

/// Sends the operations to the focused application.
pub fn send(ops: &[OutputOp]) {
    send_inputs(&build_inputs(ops));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_become_unicode_key_pairs() {
        let inputs = build_inputs(&[OutputOp::Backspace, OutputOp::Char('ệ'), OutputOp::Char('😀')]);
        // 1 backspace + 1 BMP char + 2 surrogate units, each down and up.
        assert_eq!(inputs.len(), 8);
        // SAFETY: every input built by `build_inputs` is a keyboard input.
        let first_char = unsafe { inputs[2].Anonymous.ki };
        assert_eq!(first_char.wScan, 'ệ' as u16);
        assert_eq!(first_char.dwFlags, KEYEVENTF_UNICODE);
        assert_eq!(first_char.dwExtraInfo, INJECTED_MARKER);
        // SAFETY: as above.
        let backspace_up = unsafe { inputs[1].Anonymous.ki };
        assert_eq!(backspace_up.wVk, VK_BACK);
        assert_eq!(backspace_up.dwFlags, KEYEVENTF_KEYUP);
    }

    #[test]
    fn replayed_extended_release_keeps_flags_and_marker() {
        // SAFETY: `replay_input` builds a keyboard input.
        let event = unsafe {
            replay_input(0x2E, 0x53, LLKHF_EXTENDED | LLKHF_UP)
                .Anonymous
                .ki
        };
        assert_eq!((event.wVk, event.wScan), (0x2E, 0x53));
        assert_eq!(event.dwFlags, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP);
        assert_eq!(event.dwExtraInfo, INJECTED_MARKER);
    }

    #[test]
    fn replayed_packet_becomes_unicode_input() {
        // SAFETY: `replay_input` builds a keyboard input.
        let event = unsafe { replay_input(u32::from(VK_PACKET), 0x1EC7, 0).Anonymous.ki };
        assert_eq!((event.wVk, event.wScan), (0, 0x1EC7));
        assert_eq!(event.dwFlags, KEYEVENTF_UNICODE);
    }
}
