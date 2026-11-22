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
- Gõ qua clipboard: dán chữ thay vì gửi phím Unicode, cho các ứng dụng Linux
  chạy qua WSLg (và RemoteApp) vốn chỉ nhận mã phím. Chế độ *Tự động* chỉ bật
  cho cửa sổ của `msrdc.exe`; *Luôn dùng* áp dụng cho mọi ứng dụng. Phím dán
  chọn được (Ctrl+V, Ctrl+Shift+V cho terminal, Shift+Insert). Nội dung
  clipboard cũ được khôi phục sau khi gõ và chữ dán không lưu vào lịch sử
  clipboard của Windows.
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
    phát lại kết quả bằng `SendInput` và mở cửa sổ cài đặt. Nhấn đúp biểu tượng
    khay để mở lại cài đặt.
  - `rustkey.exe --background`: chỉ chạy ở khay, không mở cài đặt (dùng khi
    khởi động cùng Windows).
  - `rustkey.exe --settings`: cửa sổ cài đặt WinUI 3 (tiến trình riêng), một
    màn hình duy nhất rộng cố định 700px, có nút gạt Việt/Anh. Khi lưu, nó ghi
    file cấu hình rồi báo cho tiến trình khay nạp lại; ngược lại cửa sổ tự cập
    nhật khi khay đổi cấu hình (ví dụ chuyển Việt/Anh bằng phím tắt).
  - `bundle.rs`: giải nén Windows App Runtime nhúng trong exe cho cửa sổ cài đặt.
  - `clipboard.rs`: luồng gõ qua clipboard. Chữ được đưa lên clipboard theo
    kiểu *delayed rendering* và chỉ trả dữ liệu cho tiến trình đích (bỏ qua các
    trình theo dõi clipboard). Trong lúc chờ dán, các phím tiếp theo được giữ
    lại rồi phát lại đúng thứ tự.

Cấu hình lưu tại `%APPDATA%\RustKey\settings.json`, gõ tắt tại
`%APPDATA%\RustKey\macros.txt` (cùng định dạng với OpenKey).

## Build & chạy

Yêu cầu build: Rust stable (edition 2024) với toolchain MSVC. Lần build đầu
`windows-reactor-setup` tải gói NuGet Windows App Runtime (cache ở
`%LOCALAPPDATA%\windows-reactor-setup`).

```powershell
cargo build --release
.\target\release\rustkey.exe            # chạy bộ gõ
.\target\release\rustkey.exe --settings # mở cài đặt
cargo test --workspace
```

### Phân phối (một file `rustkey.exe` duy nhất)

`target\release\rustkey.exe` là bản self-contained, chỉ cần copy file này sang
máy Windows 10 1809+ (x64) là chạy, không cần cài thêm gì:

- CRT được link tĩnh (`.cargo\config.toml`), không cần VC++ Redistributable.
- Windows App Runtime (WinUI 3) được nén và nhúng vào exe (`build.rs`). Lần đầu
  mở cài đặt, nó được giải nén (khoảng 1 giây) vào
  `%LOCALAPPDATA%\RustKey\runtime\<id>\` cùng một bản sao của exe; cửa sổ cài
  đặt chạy từ đó. Bộ gõ ở khay hệ thống không cần runtime này.
- Exe chưa ký số nên lần đầu chạy Windows SmartScreen có thể cảnh báo.

> Do UIPI, muốn gõ vào cửa sổ chạy quyền Administrator thì RustKey cũng phải
> chạy với quyền Administrator.

## Giấy phép

GPL-3.0-only, cùng giấy phép với OpenKey. Biểu tượng lấy từ OpenKey.
