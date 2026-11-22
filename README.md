# RustKey

Bộ gõ tiếng Việt viết bằng Rust, giao diện cài đặt WinUI 3 (dùng
[`windows-reactor`](https://docs.rs/windows-reactor)). Thuật toán gõ được port
từ [OpenKey](https://github.com/tuyenvm/OpenKey) của Mai Vũ Tuyên.

## Tính năng

- Kiểu gõ Telex, VNI, Simple Telex 1/2; đặt dấu kiểu mới/cũ, gõ tự do.
- Bảng mã Unicode, TCVN3 (ABC), VNI Windows, Unicode tổ hợp, Vietnamese Locale CP1258.
- Kiểm tra chính tả, tự khôi phục từ sai, tạm tắt chính tả/bộ gõ bằng Ctrl/Alt.
- Gõ tắt (macro), tự viết hoa macro, viết hoa chữ đầu câu.
- Phím tắt chuyển Việt/Anh tuỳ chỉnh (mặc định Ctrl + Shift), có âm báo.
- Chuyển thông minh: nhớ ngôn ngữ (và bảng mã) theo từng ứng dụng.
- Biểu tượng khay hệ thống với menu nhanh; khởi động cùng Windows.

## Cấu trúc

```
crates/
  rustkey-engine/    Engine độc lập nền tảng (không phụ thuộc OS, không unsafe)
  rustkey-windows/   Ứng dụng Windows: hook bàn phím, khay hệ thống, UI WinUI 3
```

- **`rustkey-engine`**: `Engine` là máy trạng thái port từ OpenKey; `Processor`
  bọc engine, xử lý phím tắt, macro, bảng mã và trả về danh sách `OutputOp`
  (`Backspace`, `Char`, `Key`) để bất kỳ nền tảng nào phát lại.
- **`rustkey-windows`** (file `rustkey.exe`):
  - `rustkey.exe`: chạy ở khay hệ thống, cài `WH_KEYBOARD_LL`/`WH_MOUSE_LL`,
    phát lại kết quả bằng `SendInput`.
  - `rustkey.exe --settings`: cửa sổ cài đặt WinUI 3 (tiến trình riêng). Khi
    lưu, nó ghi file cấu hình rồi báo cho tiến trình khay nạp lại.

Cấu hình lưu tại `%APPDATA%\RustKey\settings.json`, gõ tắt tại
`%APPDATA%\RustKey\macros.txt` (cùng định dạng với OpenKey).

## Build & chạy

Yêu cầu: Rust stable (edition 2024), Windows 10 1809+ và
[Windows App Runtime](https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads)
cho cửa sổ cài đặt.

```powershell
cargo build --release
.\target\release\rustkey.exe            # chạy bộ gõ
.\target\release\rustkey.exe --settings # mở cài đặt
cargo test --workspace
```

> Do UIPI, muốn gõ vào cửa sổ chạy quyền Administrator thì RustKey cũng phải
> chạy với quyền Administrator.

## Port sang hệ điều hành khác

Engine không phụ thuộc nền tảng. Một host mới chỉ cần:

1. Chuyển phím của hệ điều hành sang mã trong `rustkey_engine::keys` (giá trị
   trùng virtual-key của Windows) và gọi `Processor::handle_key(KeyInput { .. })`
   cho mọi sự kiện nhấn/nhả phím; gọi `handle_mouse_click()` khi người dùng click.
2. Nếu `Output::consumed` là `true` thì chặn phím gốc, rồi phát lại `Output::ops`
   theo thứ tự (xoá lùi, gửi ký tự Unicode, gửi phím).
3. Lưu/nạp `Config` và `MacroTable` (bật feature `serde` để dùng serde).

```rust
use rustkey_engine::{Config, KeyInput, Processor, keys};

let mut processor = Processor::new(Config::default());
let output = processor.handle_key(KeyInput::down(keys::KEY_A));
for op in &output.ops {
    // backspace / gửi ký tự / gửi phím bằng API của hệ điều hành
}
```

Ví dụ: macOS dùng `CGEventTap`, Linux dùng IBus/Fcitx5 (nên dùng preedit thay
vì backspace).

## Giấy phép

GPL-3.0-only, cùng giấy phép với OpenKey. Biểu tượng lấy từ OpenKey.
