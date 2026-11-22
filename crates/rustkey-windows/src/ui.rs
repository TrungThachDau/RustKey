//! The settings window, a WinUI 3 app built with `windows-reactor`.
//!
//! It runs in its own process (`rustkey.exe --settings`). Every change is
//! written to disk right away and the tray process is asked to reload.

use rustkey_engine::{CodeTable, Config, InputType, MacroTable, keys};
use windows_reactor::*;

use crate::settings::{self, Settings};
use crate::win::{self, WM_RUSTKEY_RELOAD};

/// Title of the settings window (also used to find an existing window).
pub const WINDOW_TITLE: &str = "RustKey – Cài đặt";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Typing,
    Spelling,
    Macros,
    System,
    About,
}

impl Page {
    const ALL: [Page; 5] = [Page::Typing, Page::Spelling, Page::Macros, Page::System, Page::About];

    fn tag(self) -> &'static str {
        match self {
            Page::Typing => "typing",
            Page::Spelling => "spelling",
            Page::Macros => "macros",
            Page::System => "system",
            Page::About => "about",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Typing => "Bộ gõ",
            Page::Spelling => "Chính tả",
            Page::Macros => "Gõ tắt",
            Page::System => "Hệ thống",
            Page::About => "Thông tin",
        }
    }

    fn symbol(self) -> Symbol {
        match self {
            Page::Typing => Symbol::Keyboard,
            Page::Spelling => Symbol::Font,
            Page::Macros => Symbol::List,
            Page::System => Symbol::Setting,
            Page::About => Symbol::Help,
        }
    }

    fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|page| page.tag() == tag)
    }
}

/// Boolean options shown as switches or check boxes.
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
    Navigate(Option<String>),
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
    page: Page,
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
}

impl Component for SettingsWindow {
    type Input = SettingsInput;
    type Message = Message;

    fn create(_input: &SettingsInput, _context: &ComponentContext<Self>) -> Self {
        let settings = Settings::load();
        let macros = settings::load_macros(settings.config.code_table);
        Self {
            page: Page::Typing,
            settings,
            macros,
            autostart: win::autostart_enabled(),
            macro_text: String::new(),
            macro_content: String::new(),
            selected_macro: None,
            error: None,
        }
    }

    fn update(&mut self, message: Message, _context: &ComponentContext<Self>) {
        match message {
            Message::Navigate(tag) => {
                if let Some(page) = tag.as_deref().and_then(Page::from_tag) {
                    self.page = page;
                }
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
            .client_size(900.0, 680.0)
            .backdrop(WindowBackdrop::Mica);
        if let Some(icon) = input.icon {
            visuals = visuals.icon(icon);
        }
        context.window_visuals(visuals);

        let menu_items = Page::ALL.map(|page| {
            (
                page.tag(),
                NavigationViewItem::new()
                    .tag(page.tag())
                    .is_selected(page == self.page)
                    .slots([
                        SlotView::new(NavigationViewItemSlot::Content, page.title()),
                        SlotView::new(
                            NavigationViewItemSlot::Icon,
                            SymbolIcon::new().symbol(page.symbol()),
                        ),
                    ]),
            )
        });

        let page = match self.page {
            Page::Typing => self.typing_page(context),
            Page::Spelling => self.spelling_page(context),
            Page::Macros => self.macros_page(context),
            Page::System => self.system_page(context),
            Page::About => about_page(),
        };

        let error: View = match &self.error {
            Some(message) => InfoBar::new()
                .title("Lỗi")
                .message(message.clone())
                .severity(InfoBarSeverity::Error)
                .is_open(true)
                .is_closable(true)
                .on_closed(context.message(Message::DismissError))
                .into(),
            None => View::empty(),
        };

        let body = StackPanel::new()
            .spacing(8.0)
            .margin(Thickness::new(32.0, 24.0, 32.0, 32.0))
            .max_width(860.0)
            .keyed_children([
                (
                    "title",
                    View::from(
                        TextBlock::new()
                            .text(self.page.title())
                            .font_size(28.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .margin(Thickness::new(0.0, 0.0, 0.0, 8.0)),
                    ),
                ),
                ("error", error),
                (self.page.tag(), page),
            ]);

        NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .is_settings_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .pane_title("RustKey")
            .open_pane_length(220.0)
            .on_selected_tag_changed(context.callback(Message::Navigate))
            .slots([
                SlotView::collection(NavigationViewSlot::MenuItems, menu_items),
                SlotView::new(NavigationViewSlot::Content, ScrollViewer::new().content(body)),
            ])
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

fn section(title: &str) -> View {
    TextBlock::new()
        .text(title)
        .font_weight(FontWeight::SEMI_BOLD)
        .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
        .into()
}

fn labelled(title: &str, description: &str, control: View) -> View {
    let text = StackPanel::new()
        .spacing(2.0)
        .vertical_alignment(VerticalAlignment::Center)
        .grid_column(0)
        .children((
        TextBlock::new().text(title).text_wrapping(TextWrapping::Wrap),
        TextBlock::new()
            .text(description)
            .font_size(12.0)
            .opacity(0.7)
            .text_wrapping(TextWrapping::Wrap),
    ));
    card(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .children((
                text,
                Border::new()
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(control),
            )),
    )
}

impl SettingsWindow {
    fn switch(
        &self,
        context: &ViewContext<Self>,
        flag: Flag,
        title: &str,
        description: &str,
        enabled: bool,
    ) -> View {
        let toggle = ToggleSwitch::new()
            .is_on(flag.get(&self.settings.config))
            .is_enabled(enabled)
            .on_toggled(context.callback(move |value| Message::Flag(flag, value)))
            .slots([
                SlotView::new(ToggleSwitchSlot::OnContent, "Bật"),
                SlotView::new(ToggleSwitchSlot::OffContent, "Tắt"),
            ]);
        labelled(title, description, toggle)
    }

    fn check(&self, context: &ViewContext<Self>, flag: Flag, text: &str) -> View {
        CheckBox::new()
            .is_checked(flag.get(&self.settings.config))
            .on_is_checked_changed(context.callback(move |value| Message::Flag(flag, value)))
            .min_width(0.0)
            .content(text)
    }

    fn typing_page(&self, context: &ViewContext<Self>) -> View {
        let config = &self.settings.config;
        let input_type = ComboBox::new()
            .items_source(InputType::ALL.map(InputType::name))
            .selected_index(InputType::ALL.iter().position(|t| *t == config.input_type))
            .on_selection_changed(context.callback(Message::InputType))
            .min_width(220.0)
            .into();
        let code_table = ComboBox::new()
            .items_source(CodeTable::ALL.map(CodeTable::name))
            .selected_index(config.code_table.index())
            .on_selection_changed(context.callback(Message::CodeTable))
            .min_width(220.0)
            .into();

        let keys = hotkey_keys();
        let hotkey = card(
            StackPanel::new().spacing(12.0).children((
                StackPanel::new().spacing(2.0).children((
                    TextBlock::new().text("Phím tắt chuyển Việt / Anh"),
                    TextBlock::new()
                        .text(format!("Hiện tại: {}", config.switch_hotkey))
                        .font_size(12.0)
                        .opacity(0.7),
                )),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(16.0)
                    .children((
                        self.check(context, Flag::HotkeyControl, "Ctrl"),
                        self.check(context, Flag::HotkeyAlt, "Alt"),
                        self.check(context, Flag::HotkeyShift, "Shift"),
                        self.check(context, Flag::HotkeyMeta, "Win"),
                        ComboBox::new()
                            .items_source(keys.iter().map(|key| hotkey_key_name(*key)))
                            .selected_index(keys.iter().position(|key| *key == config.switch_hotkey.key))
                            .on_selection_changed(context.callback(Message::HotkeyKey))
                            .vertical_alignment(VerticalAlignment::Center)
                            .min_width(120.0),
                        self.check(context, Flag::HotkeyBeep, "Phát tiếng bíp"),
                    )),
            )),
        );

        StackPanel::new().spacing(6.0).children((
            labelled("Kiểu gõ", "Telex, VNI hoặc Telex giản lược.", input_type),
            labelled("Bảng mã", "Bảng mã của văn bản được gửi tới ứng dụng.", code_table),
            hotkey,
            section("Tuỳ chọn gõ"),
            self.switch(context, Flag::ModernOrthography, "Đặt dấu kiểu mới", "oà, uý thay vì òa, úy.", true),
            self.switch(context, Flag::FreeMark, "Bỏ dấu tự do", "Cho phép gõ dấu ở bất kỳ vị trí nào trong từ.", true),
            self.switch(
                context,
                Flag::QuickTelex,
                "Gõ nhanh (Quick Telex)",
                "cc → ch, gg → gi, kk → kh, nn → ng, qq → qu, pp → ph, tt → th.",
                true,
            ),
            self.switch(context, Flag::QuickStartConsonant, "Phụ âm đầu nhanh", "f → ph, j → gi, w → qu.", true),
            self.switch(context, Flag::QuickEndConsonant, "Phụ âm cuối nhanh", "g → ng, h → nh, k → ch.", true),
            self.switch(
                context,
                Flag::UpperCaseFirstChar,
                "Viết hoa chữ cái đầu câu",
                "Tự động viết hoa sau dấu . ? ! và Enter.",
                true,
            ),
            self.switch(
                context,
                Flag::TempOffEngine,
                "Tạm tắt bộ gõ bằng phím Alt",
                "Nhấn và thả Alt để gõ từ hiện tại không dấu.",
                true,
            ),
            self.switch(
                context,
                Flag::FixRecommendBrowser,
                "Sửa lỗi gợi ý của trình duyệt",
                "Khắc phục lỗi mất chữ khi thanh địa chỉ tự hoàn thành.",
                true,
            ),
        ))
    }

    fn spelling_page(&self, context: &ViewContext<Self>) -> View {
        let enabled = self.settings.config.check_spelling;
        StackPanel::new().spacing(6.0).children((
            self.switch(
                context,
                Flag::CheckSpelling,
                "Kiểm tra chính tả",
                "Không biến đổi các từ không phải tiếng Việt.",
                true,
            ),
            self.switch(
                context,
                Flag::RestoreIfWrongSpelling,
                "Tự khôi phục phím với từ sai",
                "Ví dụ: gõ \"text\" sẽ giữ nguyên thay vì \"tẽt\".",
                enabled,
            ),
            self.switch(
                context,
                Flag::AllowConsonantZfwj,
                "Cho phép phụ âm z, f, w, j",
                "Chấp nhận các từ mượn bắt đầu bằng z, f, w, j.",
                enabled,
            ),
            self.switch(
                context,
                Flag::TempOffSpelling,
                "Tạm tắt kiểm tra chính tả bằng phím Ctrl",
                "Nhấn và thả Ctrl để bỏ qua kiểm tra cho từ hiện tại.",
                enabled,
            ),
        ))
    }

    fn macros_page(&self, context: &ViewContext<Self>) -> View {
        let use_macro = self.settings.config.use_macro;
        let editor = card(
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Star(2.0), GridLength::Auto])
                .column_spacing(12.0)
                .children((
                    TextBox::new()
                        .grid_column(0)
                        .text(self.macro_text.clone())
                        .placeholder_text("vd: vn")
                        .on_text_changed(context.callback(Message::MacroText))
                        .slot(TextBoxSlot::Header, "Từ viết tắt"),
                    TextBox::new()
                        .grid_column(1)
                        .text(self.macro_content.clone())
                        .placeholder_text("vd: Việt Nam")
                        .on_text_changed(context.callback(Message::MacroContent))
                        .slot(TextBoxSlot::Header, "Nội dung"),
                    StackPanel::new()
                        .grid_column(2)
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .vertical_alignment(VerticalAlignment::Bottom)
                        .children((
                            Button::new()
                                .style(ButtonStyle::Accent)
                                .is_enabled(!self.macro_text.trim().is_empty())
                                .on_click(context.message(Message::SaveMacro))
                                .content("Lưu"),
                            Button::new()
                                .is_enabled(self.macros.contains(self.macro_text.trim()))
                                .on_click(context.message(Message::RemoveMacro))
                                .content("Xoá"),
                        )),
                )),
        );

        let rows = self.macros.iter().map(|entry| {
            (
                entry.text.clone(),
                ListViewItem::new().tag(entry.text.clone()).content(
                    Grid::new()
                        .columns([GridLength::Pixel(160.0), GridLength::STAR])
                        .column_spacing(12.0)
                        .children((
                            TextBlock::new()
                                .text(entry.text.clone())
                                .font_weight(FontWeight::SEMI_BOLD)
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
            TextBlock::new()
                .text("Chưa có từ gõ tắt nào.")
                .opacity(0.7)
                .margin(Thickness::uniform(8.0))
                .into()
        } else {
            ListView::new()
                .selection_mode(ListViewSelectionMode::Single)
                .selected_index(self.selected_macro)
                .on_selection_changed(context.callback(Message::MacroSelected))
                .max_height(320.0)
                .collection_slot(ListViewSlot::Items, rows)
        };

        StackPanel::new().spacing(6.0).children((
            self.switch(context, Flag::UseMacro, "Bật gõ tắt", "Tự thay thế từ viết tắt khi gõ dấu cách.", true),
            self.switch(
                context,
                Flag::UseMacroInEnglishMode,
                "Gõ tắt cả khi ở chế độ tiếng Anh",
                "Vẫn thay thế từ viết tắt khi tắt tiếng Việt.",
                use_macro,
            ),
            self.switch(
                context,
                Flag::AutoCapsMacro,
                "Tự viết hoa theo từ viết tắt",
                "vn → Việt Nam, Vn → Việt Nam, VN → VIỆT NAM.",
                use_macro,
            ),
            section(&format!("Danh sách gõ tắt ({})", self.macros.len())),
            editor,
            card(list),
        ))
    }

    fn system_page(&self, context: &ViewContext<Self>) -> View {
        let autostart = ToggleSwitch::new()
            .is_on(self.autostart)
            .on_toggled(context.callback(Message::Autostart))
            .slots([
                SlotView::new(ToggleSwitchSlot::OnContent, "Bật"),
                SlotView::new(ToggleSwitchSlot::OffContent, "Tắt"),
            ]);
        let reset = Button::new()
            .on_click(context.message(Message::ResetDefaults))
            .content("Khôi phục");
        StackPanel::new().spacing(6.0).children((
            labelled("Khởi động cùng Windows", "Chạy RustKey khi đăng nhập.", autostart),
            self.switch(
                context,
                Flag::SmartSwitch,
                "Nhớ ngôn ngữ theo ứng dụng",
                "Tự chuyển Việt / Anh theo từng ứng dụng đã dùng.",
                true,
            ),
            self.switch(
                context,
                Flag::RememberCodeTable,
                "Nhớ bảng mã theo ứng dụng",
                "Tự chuyển bảng mã theo từng ứng dụng đã dùng.",
                true,
            ),
            labelled(
                "Khôi phục cài đặt mặc định",
                "Đưa mọi tuỳ chọn về mặc định (không xoá danh sách gõ tắt).",
                reset,
            ),
        ))
    }
}

fn about_page() -> View {
    let link = |text: &str, uri: &str| -> View {
        match HyperlinkButton::new().navigate_uri(uri) {
            Ok(button) => button.content(text),
            Err(_) => TextBlock::new().text(uri).into(),
        }
    };
    card(StackPanel::new().spacing(8.0).children((
        TextBlock::new()
            .text(format!("RustKey {}", env!("CARGO_PKG_VERSION")))
            .font_size(20.0)
            .font_weight(FontWeight::SEMI_BOLD),
        TextBlock::new()
            .text("Bộ gõ tiếng Việt viết bằng Rust, giao diện WinUI 3 (windows-reactor).")
            .text_wrapping(TextWrapping::Wrap),
        TextBlock::new()
            .text(
                "Engine gõ được chuyển từ OpenKey của Mai Vũ Tuyên và tách riêng thành \
                 thư viện rustkey-engine để có thể dùng trên các hệ điều hành khác.",
            )
            .text_wrapping(TextWrapping::Wrap),
        TextBlock::new().text("Giấy phép: GNU GPL v3.").opacity(0.7),
        link("OpenKey trên GitHub", "https://github.com/tuyenvm/OpenKey"),
        link("windows-reactor", "https://docs.rs/windows-reactor"),
    )))
}

/// Shows the settings window (blocks until it is closed).
pub fn run() -> anyhow::Result<()> {
    let icon = write_icon();
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

    #[test]
    fn pages_round_trip_tags() {
        for page in Page::ALL {
            assert_eq!(Page::from_tag(page.tag()), Some(page));
        }
    }
}
