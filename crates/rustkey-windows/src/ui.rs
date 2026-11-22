//! The settings window, a WinUI 3 app built with `windows-reactor`.
//!
//! It runs in its own process (`rustkey.exe --settings`). Every change is
//! written to disk right away and the tray process is asked to reload.
//!
//! Everything lives on a single, fixed width screen: the language switch and
//! main choices on top, the option groups side by side below and a short
//! footer.

use std::time::Duration;

use rustkey_engine::{CodeTable, Config, InputType, MacroTable, keys};
use windows_reactor::*;

use crate::settings::{self, Settings};
use crate::win::{self, WM_RUSTKEY_RELOAD};

/// Title of the settings window (also used to find an existing window).
pub const WINDOW_TITLE: &str = "RustKey";
/// Project home page linked from the footer.
const REPOSITORY_URL: &str = env!("CARGO_PKG_REPOSITORY");

/// Client size in DIPs. The width is fixed; the height fits everything
/// without scrolling (a scroll bar appears on shorter screens).
const WINDOW_SIZE: (f64, f64) = (700.0, 800.0);
/// The window can only get shorter than this, never wider or narrower.
const MIN_WINDOW_HEIGHT: f64 = 400.0;
/// How often the settings file is checked for changes made by the tray.
const WATCH_INTERVAL: Duration = Duration::from_millis(400);

/// Boolean options shown as check boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    FreeMark,
    CheckSpelling,
    ModernOrthography,
    QuickTelex,
    RestoreIfWrongSpelling,
    FixRecommendBrowser,
    UseMacro,
    UseMacroInEnglishMode,
    AutoCapsMacro,
    UpperCaseFirstChar,
    TempOffSpelling,
    TempOffEngine,
    AllowConsonantZfwj,
    QuickStartConsonant,
    QuickEndConsonant,
    SmartSwitch,
    RememberCodeTable,
    HotkeyControl,
    HotkeyAlt,
    HotkeyShift,
    HotkeyMeta,
    HotkeyBeep,
}

macro_rules! flag_field {
    ($config:expr, $flag:expr, $($borrow:tt)+) => {
        match $flag {
            Flag::FreeMark => $($borrow)+ $config.free_mark,
            Flag::CheckSpelling => $($borrow)+ $config.check_spelling,
            Flag::ModernOrthography => $($borrow)+ $config.modern_orthography,
            Flag::QuickTelex => $($borrow)+ $config.quick_telex,
            Flag::RestoreIfWrongSpelling => $($borrow)+ $config.restore_if_wrong_spelling,
            Flag::FixRecommendBrowser => $($borrow)+ $config.fix_recommend_browser,
            Flag::UseMacro => $($borrow)+ $config.use_macro,
            Flag::UseMacroInEnglishMode => $($borrow)+ $config.use_macro_in_english_mode,
            Flag::AutoCapsMacro => $($borrow)+ $config.auto_caps_macro,
            Flag::UpperCaseFirstChar => $($borrow)+ $config.upper_case_first_char,
            Flag::TempOffSpelling => $($borrow)+ $config.temp_off_spelling,
            Flag::TempOffEngine => $($borrow)+ $config.temp_off_engine,
            Flag::AllowConsonantZfwj => $($borrow)+ $config.allow_consonant_zfwj,
            Flag::QuickStartConsonant => $($borrow)+ $config.quick_start_consonant,
            Flag::QuickEndConsonant => $($borrow)+ $config.quick_end_consonant,
            Flag::SmartSwitch => $($borrow)+ $config.smart_switch,
            Flag::RememberCodeTable => $($borrow)+ $config.remember_code_table,
            Flag::HotkeyControl => $($borrow)+ $config.switch_hotkey.control,
            Flag::HotkeyAlt => $($borrow)+ $config.switch_hotkey.alt,
            Flag::HotkeyShift => $($borrow)+ $config.switch_hotkey.shift,
            Flag::HotkeyMeta => $($borrow)+ $config.switch_hotkey.meta,
            Flag::HotkeyBeep => $($borrow)+ $config.switch_hotkey.beep,
        }
    };
}

impl Flag {
    pub fn get(self, config: &Config) -> bool {
        *flag_field!(config, self, &)
    }

    pub fn set(self, config: &mut Config, value: bool) {
        *flag_field!(config, self, &mut) = value;
    }
}

/// Keys that can complete the switch hotkey: none, Space, A-Z, 0-9.
fn hotkey_keys() -> Vec<Option<u16>> {
    let mut list = vec![None, Some(keys::KEY_SPACE)];
    list.extend((0..26).map(|i| Some(keys::KEY_A + i)));
    list.extend((0..10).map(|i| Some(keys::KEY_0 + i)));
    list
}

fn hotkey_key_name(key: Option<u16>) -> String {
    match key {
        None => "(không)".to_owned(),
        Some(keys::KEY_SPACE) => "Space".to_owned(),
        Some(key) => char::from_u32(u32::from(key)).map_or_else(|| format!("0x{key:02X}"), String::from),
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Vietnamese(bool),
    /// `settings.json` was changed (e.g. the language was switched by hotkey).
    SettingsChanged,
    Flag(Flag, bool),
    InputType(Option<usize>),
    CodeTable(Option<usize>),
    HotkeyKey(Option<usize>),
    Autostart(bool),
    ResetDefaults,
    MacroSelected(Option<usize>),
    MacroText(String),
    MacroContent(String),
    SaveMacro,
    RemoveMacro,
    DismissError,
}

/// Parent supplied input: the window icon path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsInput {
    pub icon: Option<&'static str>,
}

pub struct SettingsWindow {
    settings: Settings,
    macros: MacroTable,
    autostart: bool,
    macro_text: String,
    macro_content: String,
    selected_macro: Option<usize>,
    error: Option<String>,
}

impl SettingsWindow {
    fn report<E: std::fmt::Display>(&mut self, result: Result<(), E>) {
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }

    /// Reads the latest settings (the tray may have changed the language or
    /// the per-application memory), applies `change` and saves.
    fn change_config(&mut self, change: impl FnOnce(&mut Config)) {
        let mut latest = Settings::load();
        change(&mut latest.config);
        self.settings = latest;
        let result = self.settings.save();
        self.report(result.map_err(|e| format!("{e:#}")));
        win::post_to_tray(WM_RUSTKEY_RELOAD);
    }

    fn save_macros(&mut self) {
        let result = settings::save_macros(&self.macros);
        self.report(result.map_err(|e| format!("{e:#}")));
        win::post_to_tray(WM_RUSTKEY_RELOAD);
    }

    /// Sends [`Message::SettingsChanged`] once `settings.json` is modified, so
    /// the window follows changes made by the tray while it is open.
    fn watch_settings(context: &ComponentContext<Self>) {
        let since = settings::settings_modified();
        context.spawn_background(move |cancel| {
            while !cancel.is_cancelled() && settings::settings_modified() == since {
                std::thread::sleep(WATCH_INTERVAL);
            }
            Message::SettingsChanged
        });
    }
}

impl Component for SettingsWindow {
    type Input = SettingsInput;
    type Message = Message;

    fn create(_input: &SettingsInput, context: &ComponentContext<Self>) -> Self {
        Self::watch_settings(context);
        let settings = Settings::load();
        let macros = settings::load_macros(settings.config.code_table);
        Self {
            settings,
            macros,
            autostart: win::autostart_enabled(),
            macro_text: String::new(),
            macro_content: String::new(),
            selected_macro: None,
            error: None,
        }
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {
        match message {
            Message::Vietnamese(value) => {
                if value != self.settings.config.vietnamese {
                    self.change_config(|config| config.vietnamese = value);
                }
            }
            Message::SettingsChanged => {
                self.settings = Settings::load();
                Self::watch_settings(context);
            }
            Message::Flag(flag, value) => {
                if flag.get(&self.settings.config) != value {
                    self.change_config(|config| flag.set(config, value));
                }
            }
            Message::InputType(index) => {
                if let Some(&input_type) = index.and_then(|i| InputType::ALL.get(i))
                    && input_type != self.settings.config.input_type
                {
                    self.change_config(|config| config.input_type = input_type);
                }
            }
            Message::CodeTable(index) => {
                if let Some(table) = index.and_then(CodeTable::from_index)
                    && table != self.settings.config.code_table
                {
                    self.change_config(|config| config.code_table = table);
                }
            }
            Message::HotkeyKey(index) => {
                if let Some(key) = index.and_then(|i| hotkey_keys().get(i).copied())
                    && key != self.settings.config.switch_hotkey.key
                {
                    self.change_config(|config| config.switch_hotkey.key = key);
                }
            }
            Message::Autostart(enable) => {
                if enable != self.autostart {
                    let result = win::set_autostart(enable).map_err(|e| format!("{e:#}"));
                    self.report(result);
                    self.autostart = win::autostart_enabled();
                }
            }
            Message::ResetDefaults => {
                self.change_config(|config| {
                    let vietnamese = config.vietnamese;
                    *config = Config::default();
                    config.vietnamese = vietnamese;
                });
            }
            Message::MacroSelected(index) => {
                self.selected_macro = index;
                if let Some(entry) = index.and_then(|i| self.macros.iter().nth(i)) {
                    self.macro_text = entry.text.clone();
                    self.macro_content = entry.content.clone();
                }
            }
            Message::MacroText(text) => self.macro_text = text,
            Message::MacroContent(text) => self.macro_content = text,
            Message::SaveMacro => {
                let text = self.macro_text.trim().to_owned();
                match self.macros.add(&text, &self.macro_content) {
                    Ok(()) => {
                        self.selected_macro = self.macros.iter().position(|entry| entry.text == text);
                        self.save_macros();
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Message::RemoveMacro => {
                if self.macros.remove(self.macro_text.trim()) {
                    self.selected_macro = None;
                    self.macro_text.clear();
                    self.macro_content.clear();
                    self.save_macros();
                }
            }
            Message::DismissError => self.error = None,
        }
    }

    fn view(&self, input: &SettingsInput, context: &mut ViewContext<Self>) -> View {
        context.window_title(WINDOW_TITLE);
        let mut visuals = WindowVisuals::new()
            .client_size(WINDOW_SIZE.0, WINDOW_SIZE.1)
            .constraints(WindowConstraints {
                min_width: Some(WINDOW_SIZE.0),
                min_height: Some(MIN_WINDOW_HEIGHT),
                max_width: Some(WINDOW_SIZE.0),
                max_height: None,
            })
            .backdrop(WindowBackdrop::Mica);
        if let Some(icon) = input.icon {
            visuals = visuals.icon(icon);
        }
        context.window_visuals(visuals);

        let groups = Grid::new()
            .columns([GridLength::STAR, GridLength::STAR])
            .column_spacing(12.0)
            .children((
                cell(0, 0, self.typing_group(context)),
                cell(
                    0,
                    1,
                    StackPanel::new()
                        .spacing(12.0)
                        .children((self.spelling_group(context), self.system_group(context))),
                ),
            ));

        let layout = StackPanel::new()
            .spacing(12.0)
            .margin(Thickness::xy(20.0, 16.0))
            .children((
                self.main_bar(context),
                groups,
                self.macro_group(context),
                self.footer(context),
            ));

        ScrollViewer::new().content(layout)
    }
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

fn card(content: impl Into<View>) -> View {
    Border::new()
        .padding(Thickness::xy(16.0, 12.0))
        .corner_radius(8.0)
        .border_thickness(1.0)
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .content(content)
}

/// A titled card holding a group of options.
fn group(title: &str, body: impl Into<View>) -> View {
    card(StackPanel::new().spacing(2.0).children((
        TextBlock::new()
            .text(title)
            .font_weight(FontWeight::SEMI_BOLD)
            .margin(Thickness::new(0.0, 0.0, 0.0, 6.0)),
        body.into(),
    )))
}

/// Places `content` in a grid cell.
fn cell(row: i32, column: i32, content: impl Into<View>) -> View {
    Border::new().grid_row(row).grid_column(column).content(content)
}

fn caption(text: &str) -> TextBlock {
    TextBlock::new().text(text).font_size(12.0).opacity(0.7)
}

/// Text that wraps instead of being clipped when its column gets narrow.
fn wrapping(text: &str) -> TextBlock {
    TextBlock::new().text(text).text_wrapping(TextWrapping::WrapWholeWords)
}

fn link(text: &str, uri: &str) -> View {
    match HyperlinkButton::new().navigate_uri(uri) {
        Ok(button) => button.content(text),
        Err(_) => TextBlock::new().text(uri).into(),
    }
}

impl SettingsWindow {
    /// A key cap style toggle bound to a hotkey modifier.
    fn modifier(&self, context: &ViewContext<Self>, flag: Flag, text: &str) -> View {
        ToggleButton::new()
            .is_checked(flag.get(&self.settings.config))
            .on_is_checked_changed(context.callback(move |value| Message::Flag(flag, value)))
            .min_width(0.0)
            .content(text)
    }

    /// A check box bound to `flag` with an explanatory tooltip.
    fn option(&self, context: &ViewContext<Self>, flag: Flag, text: &str, tip: &str, enabled: bool) -> View {
        CheckBox::new()
            .is_checked(flag.get(&self.settings.config))
            .is_enabled(enabled)
            .on_is_checked_changed(context.callback(move |value| Message::Flag(flag, value)))
            .min_width(0.0)
            .content(wrapping(text))
            .tooltip(tip)
    }

    /// Language switch, input method, code table and the switch hotkey.
    fn main_bar(&self, context: &ViewContext<Self>) -> View {
        let config = &self.settings.config;
        let keys = hotkey_keys();

        let language = ToggleSwitch::new()
            .is_on(config.vietnamese)
            .on_toggled(context.callback(Message::Vietnamese))
            .slots([
                SlotView::new(ToggleSwitchSlot::Header, "Chế độ gõ"),
                SlotView::new(ToggleSwitchSlot::OnContent, "Tiếng Việt"),
                SlotView::new(ToggleSwitchSlot::OffContent, "Tiếng Anh"),
            ]);
        let input_type = ComboBox::new()
            .items_source(InputType::ALL.map(InputType::name))
            .selected_index(InputType::ALL.iter().position(|t| *t == config.input_type))
            .on_selection_changed(context.callback(Message::InputType))
            .min_width(0.0)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .slot(ComboBoxSlot::Header, "Kiểu gõ");
        let code_table = ComboBox::new()
            .items_source(CodeTable::ALL.map(CodeTable::name))
            .selected_index(config.code_table.index())
            .on_selection_changed(context.callback(Message::CodeTable))
            .min_width(0.0)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .slot(ComboBoxSlot::Header, "Bảng mã");
        let hotkey = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(4.0)
            .children((
                TextBlock::new()
                    .text("Phím chuyển Việt / Anh")
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::new(0.0, 0.0, 12.0, 0.0)),
                self.modifier(context, Flag::HotkeyControl, "Ctrl"),
                self.modifier(context, Flag::HotkeyAlt, "Alt"),
                self.modifier(context, Flag::HotkeyShift, "Shift"),
                self.modifier(context, Flag::HotkeyMeta, "Win"),
                TextBlock::new()
                    .text("+")
                    .opacity(0.7)
                    .vertical_alignment(VerticalAlignment::Center)
                    .margin(Thickness::xy(4.0, 0.0)),
                ComboBox::new()
                    .items_source(keys.iter().map(|key| hotkey_key_name(*key)))
                    .selected_index(keys.iter().position(|key| *key == config.switch_hotkey.key))
                    .on_selection_changed(context.callback(Message::HotkeyKey))
                    .vertical_alignment(VerticalAlignment::Center)
                    .min_width(0.0)
                    .width(96.0),
            ));

        card(
            Grid::new()
                .columns([GridLength::STAR, GridLength::STAR, GridLength::STAR])
                .rows([GridLength::Auto, GridLength::Auto])
                .column_spacing(20.0)
                .row_spacing(14.0)
                .children((
                    cell(0, 0, language),
                    cell(0, 1, input_type),
                    cell(0, 2, code_table),
                    Border::new().grid_row(1).grid_column_span(3).content(hotkey),
                )),
        )
    }

    fn typing_group(&self, context: &ViewContext<Self>) -> View {
        group(
            "Gõ",
            StackPanel::new().children((
                self.option(context, Flag::ModernOrthography, "Đặt dấu kiểu mới", "oà, uý thay vì òa, úy.", true),
                self.option(context, Flag::FreeMark, "Bỏ dấu tự do", "Gõ dấu ở bất kỳ vị trí nào trong từ.", true),
                self.option(
                    context,
                    Flag::QuickTelex,
                    "Gõ nhanh (Quick Telex)",
                    "cc → ch, gg → gi, kk → kh, nn → ng, qq → qu, pp → ph, tt → th.",
                    true,
                ),
                self.option(context, Flag::QuickStartConsonant, "Phụ âm đầu nhanh", "f → ph, j → gi, w → qu.", true),
                self.option(context, Flag::QuickEndConsonant, "Phụ âm cuối nhanh", "g → ng, h → nh, k → ch.", true),
                self.option(
                    context,
                    Flag::UpperCaseFirstChar,
                    "Viết hoa đầu câu",
                    "Tự viết hoa sau dấu . ? ! và Enter.",
                    true,
                ),
                self.option(
                    context,
                    Flag::TempOffEngine,
                    "Tạm tắt bằng phím Alt",
                    "Nhấn và thả Alt để gõ từ hiện tại không dấu.",
                    true,
                ),
                self.option(
                    context,
                    Flag::FixRecommendBrowser,
                    "Sửa lỗi gợi ý trình duyệt",
                    "Khắc phục lỗi mất chữ khi thanh địa chỉ tự hoàn thành.",
                    true,
                ),
            )),
        )
    }

    fn spelling_group(&self, context: &ViewContext<Self>) -> View {
        let enabled = self.settings.config.check_spelling;
        group(
            "Chính tả",
            StackPanel::new().children((
                self.option(
                    context,
                    Flag::CheckSpelling,
                    "Kiểm tra chính tả",
                    "Không biến đổi các từ không phải tiếng Việt.",
                    true,
                ),
                self.option(
                    context,
                    Flag::RestoreIfWrongSpelling,
                    "Khôi phục từ sai",
                    "Gõ \"text\" sẽ giữ nguyên thay vì \"tẽt\".",
                    enabled,
                ),
                self.option(
                    context,
                    Flag::AllowConsonantZfwj,
                    "Cho phép z, f, w, j",
                    "Chấp nhận các từ mượn bắt đầu bằng z, f, w, j.",
                    enabled,
                ),
                self.option(
                    context,
                    Flag::TempOffSpelling,
                    "Tạm tắt bằng phím Ctrl",
                    "Nhấn và thả Ctrl để bỏ qua kiểm tra cho từ hiện tại.",
                    enabled,
                ),
            )),
        )
    }

    fn system_group(&self, context: &ViewContext<Self>) -> View {
        let autostart = CheckBox::new()
            .is_checked(self.autostart)
            .on_is_checked_changed(context.callback(Message::Autostart))
            .min_width(0.0)
            .content(wrapping("Khởi động cùng Windows"));
        group(
            "Hệ thống",
            StackPanel::new().children((
                autostart,
                self.option(
                    context,
                    Flag::HotkeyBeep,
                    "Bíp khi chuyển Việt / Anh",
                    "Phát tiếng bíp khi đổi ngôn ngữ bằng phím tắt.",
                    true,
                ),
                self.option(
                    context,
                    Flag::SmartSwitch,
                    "Nhớ ngôn ngữ theo ứng dụng",
                    "Tự chuyển Việt / Anh theo từng ứng dụng đã dùng.",
                    true,
                ),
                self.option(
                    context,
                    Flag::RememberCodeTable,
                    "Nhớ bảng mã theo ứng dụng",
                    "Tự chuyển bảng mã theo từng ứng dụng đã dùng.",
                    true,
                ),
            )),
        )
    }

    fn macro_group(&self, context: &ViewContext<Self>) -> View {
        let use_macro = self.settings.config.use_macro;

        let editor = Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Star(2.0), GridLength::Auto, GridLength::Auto])
            .column_spacing(6.0)
            .margin(Thickness::new(0.0, 0.0, 0.0, 6.0))
            .children((
                TextBox::new()
                    .grid_column(0)
                    .min_width(0.0)
                    .text(self.macro_text.clone())
                    .placeholder_text("Viết tắt")
                    .on_text_changed(context.callback(Message::MacroText)),
                TextBox::new()
                    .grid_column(1)
                    .min_width(0.0)
                    .text(self.macro_content.clone())
                    .placeholder_text("Nội dung")
                    .on_text_changed(context.callback(Message::MacroContent)),
                Button::new()
                    .grid_column(2)
                    .style(ButtonStyle::Accent)
                    .is_enabled(!self.macro_text.trim().is_empty())
                    .on_click(context.message(Message::SaveMacro))
                    .content("Lưu"),
                Button::new()
                    .grid_column(3)
                    .is_enabled(self.macros.contains(self.macro_text.trim()))
                    .on_click(context.message(Message::RemoveMacro))
                    .content("Xoá"),
            ));

        let rows = self.macros.iter().map(|entry| {
            (
                entry.text.clone(),
                ListViewItem::new().tag(entry.text.clone()).content(
                    Grid::new()
                        .columns([GridLength::Pixel(90.0), GridLength::STAR])
                        .column_spacing(8.0)
                        .children((
                            TextBlock::new()
                                .text(entry.text.clone())
                                .font_weight(FontWeight::SEMI_BOLD)
                                .text_trimming(TextTrimming::CharacterEllipsis)
                                .grid_column(0),
                            TextBlock::new()
                                .text(entry.content.clone())
                                .text_trimming(TextTrimming::CharacterEllipsis)
                                .grid_column(1),
                        )),
                ),
            )
        });
        let list: View = if self.macros.is_empty() {
            caption("Chưa có từ gõ tắt nào.").margin(Thickness::uniform(4.0)).into()
        } else {
            ListView::new()
                .selection_mode(ListViewSelectionMode::Single)
                .selected_index(self.selected_macro)
                .on_selection_changed(context.callback(Message::MacroSelected))
                .max_height(160.0)
                .collection_slot(ListViewSlot::Items, rows)
        };

        let options = StackPanel::new().children((
            self.option(
                context,
                Flag::UseMacro,
                "Bật gõ tắt",
                "Tự thay thế từ viết tắt khi gõ dấu cách.",
                true,
            ),
            self.option(
                context,
                Flag::UseMacroInEnglishMode,
                "Gõ tắt cả khi ở chế độ tiếng Anh",
                "Vẫn thay thế từ viết tắt khi tắt tiếng Việt.",
                use_macro,
            ),
            self.option(
                context,
                Flag::AutoCapsMacro,
                "Tự viết hoa theo từ viết tắt",
                "vn → Việt Nam, Vn → Việt Nam, VN → VIỆT NAM.",
                use_macro,
            ),
        ));

        group(
            &format!("Gõ tắt ({})", self.macros.len()),
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Star(1.6)])
                .column_spacing(16.0)
                .children((
                    cell(0, 0, options),
                    cell(0, 1, StackPanel::new().children((editor, list))),
                )),
        )
    }

    /// Version, credits and reset; an error replaces the credits when present.
    fn footer(&self, context: &ViewContext<Self>) -> View {
        let left: View = match &self.error {
            Some(message) => InfoBar::new()
                .title("Lỗi")
                .message(message.clone())
                .severity(InfoBarSeverity::Error)
                .is_open(true)
                .is_closable(true)
                .on_closed(context.message(Message::DismissError))
                .into(),
            None => StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(4.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    caption(&format!("Phiên bản {}", env!("CARGO_PKG_VERSION")))
                        .vertical_alignment(VerticalAlignment::Center),
                    caption("·").vertical_alignment(VerticalAlignment::Center),
                    link("GitHub", REPOSITORY_URL),
                )),
        };
        let reset = Button::new()
            .style(ButtonStyle::Subtle)
            .on_click(context.message(Message::ResetDefaults))
            .content("Khôi phục mặc định")
            .tooltip("Đưa mọi tuỳ chọn về mặc định (không xoá danh sách gõ tắt).");

        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(12.0)
            .children((cell(0, 0, left), cell(0, 1, reset)))
    }
}

/// Shows the settings window (blocks until it is closed).
pub fn run() -> anyhow::Result<()> {
    let icon = write_icon();
    std::thread::spawn(|| win::prepare_window_when_shown(WINDOW_TITLE, WINDOW_SIZE.0, WINDOW_SIZE.1));
    App::run_component::<SettingsWindow>(SettingsInput { icon })?;
    Ok(())
}

/// Writes the application icon next to the settings so WinUI can load it by path.
fn write_icon() -> Option<&'static str> {
    const ICON: &[u8] = include_bytes!("../assets/icon.ico");
    let path = settings::data_dir().ok()?.join("icon.ico");
    let up_to_date = std::fs::metadata(&path).is_ok_and(|m| m.len() == ICON.len() as u64);
    if !up_to_date {
        std::fs::create_dir_all(path.parent()?).ok()?;
        std::fs::write(&path, ICON).ok()?;
    }
    let path = path.to_str()?.to_owned();
    Some(Box::leak(path.into_boxed_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_FLAGS: [Flag; 22] = [
        Flag::FreeMark,
        Flag::CheckSpelling,
        Flag::ModernOrthography,
        Flag::QuickTelex,
        Flag::RestoreIfWrongSpelling,
        Flag::FixRecommendBrowser,
        Flag::UseMacro,
        Flag::UseMacroInEnglishMode,
        Flag::AutoCapsMacro,
        Flag::UpperCaseFirstChar,
        Flag::TempOffSpelling,
        Flag::TempOffEngine,
        Flag::AllowConsonantZfwj,
        Flag::QuickStartConsonant,
        Flag::QuickEndConsonant,
        Flag::SmartSwitch,
        Flag::RememberCodeTable,
        Flag::HotkeyControl,
        Flag::HotkeyAlt,
        Flag::HotkeyShift,
        Flag::HotkeyMeta,
        Flag::HotkeyBeep,
    ];

    #[test]
    fn flags_toggle_independent_fields() {
        for flag in ALL_FLAGS {
            let mut config = Config::default();
            let before = flag.get(&config);
            flag.set(&mut config, !before);
            assert_eq!(flag.get(&config), !before, "{flag:?}");
            let changed = ALL_FLAGS
                .iter()
                .filter(|other| other.get(&config) != other.get(&Config::default()))
                .count();
            assert_eq!(changed, 1, "{flag:?} changed other fields");
        }
    }

    #[test]
    fn hotkey_key_names() {
        assert_eq!(hotkey_key_name(None), "(không)");
        assert_eq!(hotkey_key_name(Some(keys::KEY_SPACE)), "Space");
        assert_eq!(hotkey_key_name(Some(keys::KEY_Z)), "Z");
        assert_eq!(hotkey_keys().len(), 38);
    }
}
