#![cfg_attr(windows, windows_subsystem = "windows")]
#![allow(unused_must_use)]

mod capture;
mod geom;
mod media;
mod outline;
mod region;
mod settings;
mod theme;
mod ui;

pub(crate) use media::AudioSettings;
use media::{Format, Profile, Rect};
pub(crate) use settings::{test_mode, wide, CaptureSettings, Settings};
use std::path::PathBuf;
use std::time::Instant;
use ui::*;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Controls::{
    TaskDialogIndirect, TASKDIALOGCONFIG, TASKDIALOG_NOTIFICATIONS, TDCBF_OK_BUTTON,
    TDF_ALLOW_DIALOG_CANCELLATION, TDF_ENABLE_HYPERLINKS, TDF_POSITION_RELATIVE_TO_WINDOW,
    TDF_SIZE_TO_CONTENT, TDN_HYPERLINK_CLICKED,
};
#[cfg(test)]
use windows::Win32::UI::Controls::{TDF_CALLBACK_TIMER, TDN_TIMER};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const CLASS: PCWSTR = w!("ezcap.window");
const TITLE: PCWSTR = w!("ezcap");
const WM_TRAY: u32 = WM_APP + 1;
const WM_REGION: u32 = WM_APP + 2;
const WM_ACTIVATE_APP: u32 = WM_APP + 3;
const TIMER_ID: usize = 1;
const ID_TRAY_AREA: i32 = 200;
const ID_TRAY_SHOT: i32 = 201;
const ID_TRAY_VIDEO: i32 = 202;
const ID_TRAY_AUDIO: i32 = 203;
const ID_TRAY_STOP: i32 = 204;
const ID_TRAY_EXIT: i32 = 205;
const ID_TRAY_OPEN: i32 = 206;

static mut APP: *mut App = std::ptr::null_mut();

const REPOSITORY: &str = "https://github.com/machine202502/ezcap";

/// The executable's icon resource. Windows identifies it by the integer 1, not by a string.
#[allow(clippy::manual_dangling_ptr)]
pub(crate) fn module_icon() -> PCWSTR {
    PCWSTR(1 as *const u16)
}

#[cfg(test)]
static CAPTURE_ABOUT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static ABOUT_SHOT: std::sync::Mutex<Option<media::Frame>> = std::sync::Mutex::new(None);

fn show_about(owner: HWND) {
    let title = wide("ezcap");
    let instruction = wide("ezcap");
    let content = wide(&format!(
        "Version {}\n\nScreen, region and audio capture.\n\n<a href=\"{REPOSITORY}\">GitHub</a>",
        env!("CARGO_PKG_VERSION")
    ));
    let mut config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        hwndParent: owner,
        ..Default::default()
    };
    #[cfg(test)]
    let flags = {
        let mut flags = TDF_ENABLE_HYPERLINKS
            | TDF_ALLOW_DIALOG_CANCELLATION
            | TDF_POSITION_RELATIVE_TO_WINDOW
            | TDF_SIZE_TO_CONTENT;
        if CAPTURE_ABOUT.load(std::sync::atomic::Ordering::SeqCst) {
            flags |= TDF_CALLBACK_TIMER;
        }
        flags
    };
    #[cfg(not(test))]
    let flags = TDF_ENABLE_HYPERLINKS
        | TDF_ALLOW_DIALOG_CANCELLATION
        | TDF_POSITION_RELATIVE_TO_WINDOW
        | TDF_SIZE_TO_CONTENT;
    config.dwFlags = flags;
    config.dwCommonButtons = TDCBF_OK_BUTTON;
    config.pszWindowTitle = PCWSTR(title.as_ptr());
    config.pszMainInstruction = PCWSTR(instruction.as_ptr());
    config.pszContent = PCWSTR(content.as_ptr());
    config.pfCallback = Some(about_link);
    unsafe {
        TaskDialogIndirect(&config, None, None, None).ok();
    }
}

#[cfg(test)]
pub(crate) fn capture_about_dialog(owner: HWND) -> media::Frame {
    ABOUT_SHOT.lock().unwrap().take();
    CAPTURE_ABOUT.store(true, std::sync::atomic::Ordering::SeqCst);
    show_about(owner);
    CAPTURE_ABOUT.store(false, std::sync::atomic::Ordering::SeqCst);
    ABOUT_SHOT
        .lock()
        .unwrap()
        .take()
        .expect("the about dialog was not captured")
}

#[cfg(test)]
fn print_window_frame(hwnd: HWND) -> media::Frame {
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    unsafe {
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).unwrap();
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let window_dc = GetWindowDC(hwnd);
        let memory = CreateCompatibleDC(window_dc);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let bitmap =
            CreateDIBSection(window_dc, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let old = SelectObject(memory, bitmap);
        PrintWindow(hwnd, memory, PRINT_WINDOW_FLAGS(2)).ok();
        let mut data = vec![0u8; width as usize * height as usize * 4];
        std::ptr::copy_nonoverlapping(bits.cast::<u8>(), data.as_mut_ptr(), data.len());
        for pixel in data.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        SelectObject(memory, old);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(hwnd, window_dc);
        media::Frame {
            width: width as u32,
            height: height as u32,
            data,
        }
    }
}

unsafe extern "system" fn about_link(
    hwnd: HWND,
    msg: TASKDIALOG_NOTIFICATIONS,
    _wparam: WPARAM,
    lparam: LPARAM,
    _: isize,
) -> windows::core::HRESULT {
    #[cfg(test)]
    if msg == TDN_TIMER
        && _wparam.0 >= 200
        && CAPTURE_ABOUT.load(std::sync::atomic::Ordering::SeqCst)
    {
        let mut shot = ABOUT_SHOT.lock().unwrap();
        if shot.is_none() {
            *shot = Some(print_window_frame(hwnd));
            drop(shot);
            PostMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)).ok();
        }
    }
    if msg == TDN_HYPERLINK_CLICKED {
        ShellExecuteW(
            hwnd,
            w!("open"),
            PCWSTR(lparam.0 as *const u16),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
    windows::core::HRESULT(0)
}

pub(crate) fn history_stamp(
    day: u16,
    month: u16,
    year: u16,
    hour: u16,
    minute: u16,
    second: u16,
) -> String {
    let name = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .get(month.saturating_sub(1) as usize)
    .copied()
    .unwrap_or("?");
    format!("{day} {name} {year}, {hour:02}:{minute:02}:{second:02}")
}

fn accepts_command(mode: Mode, id: i32, region_open: bool) -> bool {
    let screenshot = matches!(
        id,
        ID_SHOT_SCREEN | ID_SHOT_REGION | ID_TRAY_SHOT | ID_TRAY_AREA
    );
    if screenshot && (region_open || mode == Mode::Saving || mode == Mode::Selecting) {
        return false;
    }
    mode == Mode::Idle
        || (mode == Mode::Recording && screenshot)
        || matches!(
            id,
            ID_STOP | ID_TRAY_STOP | ID_TRAY_EXIT | ID_TRAY_OPEN | ID_STATUS | ID_ABOUT
        )
        || ui::history_id(id)
}

fn hide_while_active(from_window: bool, setting_on: bool) -> bool {
    from_window && setting_on
}

fn tray_item_enabled(mode: Mode, id: i32) -> bool {
    match id {
        ID_TRAY_AREA | ID_TRAY_SHOT => mode == Mode::Idle || mode == Mode::Recording,
        ID_TRAY_VIDEO | ID_TRAY_AUDIO => mode == Mode::Idle,
        ID_TRAY_STOP => mode == Mode::Recording,
        _ => true,
    }
}

#[cfg(test)]
fn test_mark_recording(app: &mut App) {
    app.mode = Mode::Recording;
    app.session = None;
    app.update_state();
}

fn history_line_now() -> String {
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    history_stamp(
        time.wDay,
        time.wMonth,
        time.wYear,
        time.wHour,
        time.wMinute,
        time.wSecond,
    )
}

fn app<'a>() -> &'a mut App {
    unsafe { &mut *APP }
}

pub(crate) fn notify_region_closed() {
    unsafe {
        if !APP.is_null() {
            PostMessageW((*APP).hwnd, WM_REGION, WPARAM(0), LPARAM(0)).ok();
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Idle,
    Selecting,
    Recording,
    Saving,
}

impl Mode {
    fn activity(self) -> geom::Activity {
        match self {
            Self::Idle => geom::Activity::Idle,
            Self::Selecting => geom::Activity::Selecting,
            Self::Recording => geom::Activity::Recording,
            Self::Saving => geom::Activity::Saving,
        }
    }
}

fn low(value: usize) -> u16 {
    (value & 0xffff) as u16
}

fn high(value: usize) -> u16 {
    ((value >> 16) & 0xffff) as u16
}

fn command_id(wparam: usize) -> i32 {
    low(wparam) as i32
}

fn notification(wparam: usize) -> u16 {
    high(wparam)
}

struct App {
    hwnd: HWND,
    controls: Controls,
    settings: Settings,
    mode: Mode,
    session: Option<media::Session>,
    finalizer: Option<media::Finalizer>,
    started: Option<Instant>,
    saving_started: Option<Instant>,
    tray: NOTIFYICONDATAW,
    tray_menu: HMENU,
    base_icon: HICON,
    status_icon: HICON,
    icon_mode: Option<Mode>,
    background: HBRUSH,
    restore_visible: bool,
    hide_region: bool,
    quit_after_save: bool,
    last_second: u64,
    outline: outline::RegionOutline,
    record_rect: Option<Rect>,
    saved_path: Option<PathBuf>,
    history: Vec<(PathBuf, String)>,
    menu_icons: Vec<HBITMAP>,
}

impl App {
    fn new(hwnd: HWND) -> Self {
        let settings = Settings::load();
        Self {
            hwnd,
            controls: Controls::new(hwnd),
            settings,
            mode: Mode::Idle,
            session: None,
            finalizer: None,
            started: None,
            saving_started: None,
            tray: NOTIFYICONDATAW::default(),
            tray_menu: HMENU::default(),
            base_icon: HICON::default(),
            status_icon: HICON::default(),
            icon_mode: None,
            background: unsafe { CreateSolidBrush(BACKGROUND) },
            restore_visible: false,
            hide_region: true,
            quit_after_save: false,
            last_second: u64::MAX,
            outline: outline::RegionOutline::new(),
            record_rect: None,
            saved_path: None,
            history: Vec::new(),
            menu_icons: Vec::new(),
        }
    }

    fn build(&mut self) {
        self.controls.build(&mut self.settings);
        self.build_tray();
        self.update_state();
    }

    fn build_tray(&mut self) {
        self.tray_menu = unsafe { CreatePopupMenu() }.unwrap();
        let mut items = vec![(ID_TRAY_OPEN, "Open window", MenuGlyph::Window)];
        unsafe {
            AppendMenuW(
                self.tray_menu,
                MF_STRING,
                ID_TRAY_OPEN as usize,
                w!("Open window"),
            )
        }
        .unwrap();
        unsafe { AppendMenuW(self.tray_menu, MF_SEPARATOR, 0, None) }.unwrap();
        for (id, text, glyph) in [
            (ID_TRAY_AREA, "Select area", MenuGlyph::Area),
            (ID_TRAY_SHOT, "Screenshot screen", MenuGlyph::Shot),
            (ID_TRAY_VIDEO, "Capture screen", MenuGlyph::Video),
            (ID_TRAY_AUDIO, "Capture audio", MenuGlyph::Audio),
            (ID_TRAY_STOP, "Stop capture", MenuGlyph::Stop),
        ] {
            unsafe {
                AppendMenuW(
                    self.tray_menu,
                    MF_STRING,
                    id as usize,
                    PCWSTR(wide(text).as_ptr()),
                )
            }
            .unwrap();
            items.push((id, text, glyph));
        }
        unsafe { AppendMenuW(self.tray_menu, MF_SEPARATOR, 0, None) }.unwrap();
        unsafe { AppendMenuW(self.tray_menu, MF_STRING, ID_TRAY_EXIT as usize, w!("Exit")) }
            .unwrap();
        items.push((ID_TRAY_EXIT, "Exit", MenuGlyph::Exit));
        for (id, _, glyph) in items {
            let bitmap = menu_bitmap(glyph);
            self.menu_icons.push(bitmap);
            let info = MENUITEMINFOW {
                cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_BITMAP,
                hbmpItem: bitmap,
                ..Default::default()
            };
            unsafe {
                SetMenuItemInfoW(self.tray_menu, id as u32, false, &info).ok();
            }
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: unsafe { LoadIconW(GetModuleHandleW(None).unwrap(), module_icon()) }
                .unwrap_or_default(),
            ..Default::default()
        };
        nid.szTip[..wide("ezcap").len()].copy_from_slice(&wide("ezcap"));
        unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.ok();
        self.base_icon = nid.hIcon;
        self.status_icon = nid.hIcon;
        self.tray = nid;
    }

    fn set_tray_status(&mut self, text: &str) {
        self.tray.szTip = [0; 128];
        let text = wide(text);
        self.tray.szTip[..text.len()].copy_from_slice(&text);
        self.tray.uFlags = NIF_TIP | NIF_ICON;
        if self.icon_mode != Some(self.mode) {
            let icon = tray_status_icon(self.base_icon, self.mode);
            if self.status_icon != self.base_icon {
                unsafe { DestroyIcon(self.status_icon) }.ok();
            }
            self.status_icon = icon;
            self.icon_mode = Some(self.mode);
        }
        self.tray.hIcon = self.status_icon;
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &self.tray) }.ok();
    }

    fn command(&mut self, id: i32, notification: u16) {
        if id == ID_TAB || id == ID_SETTINGS_TAB {
            if self.mode == Mode::Idle {
                self.controls.show_page(if id == ID_TAB { 0 } else { 1 });
            }
            return;
        }
        if id == ID_MIC_DEVICE
            || id == ID_DYN_AREA_DEVICE
            || id == ID_DYN_VIDEO_DEVICE
            || id == ID_DYN_AUDIO_DEVICE
        {
            if notification == CBN_DROPDOWN as u16 {
                self.controls.refresh_devices(id);
            }
            if notification == CBN_SELCHANGE as u16 {
                self.controls.read_ui(&mut self.settings);
            }
            return;
        }
        if !accepts_command(self.mode, id, region::active()) {
            return;
        }
        match id {
            ID_REFRESH => self.controls.refresh_all_devices(&mut self.settings),
            ID_ABOUT => show_about(self.hwnd),
            ID_PRESET if notification == CBN_SELCHANGE as u16 => {
                self.controls.apply_preset(&mut self.settings)
            }
            ID_DYN_AREA_PRESET if notification == CBN_SELCHANGE as u16 => {
                self.controls.apply_dynamic_preset(&mut self.settings, true)
            }
            ID_DYN_VIDEO_PRESET if notification == CBN_SELCHANGE as u16 => self
                .controls
                .apply_dynamic_preset(&mut self.settings, false),
            110..=112 => {
                self.controls.format_index = (id - ID_FORMAT) as usize;
                self.settings.ui.format = Format::from_index((id - ID_FORMAT) as usize);
                self.controls.load_main_video(&self.settings.ui);
                self.controls.read_ui(&mut self.settings);
                self.controls.refresh_formats();
            }
            160..=162 => {
                self.controls.dyn_area_format_index = (id - ID_DYN_AREA_FORMAT) as usize;
                self.settings.dyn_area.format =
                    Format::from_index((id - ID_DYN_AREA_FORMAT) as usize);
                self.controls.load_dynamic(&self.settings, true);
                self.controls.read_ui(&mut self.settings);
                self.controls.refresh_formats();
            }
            170..=172 => {
                self.controls.dyn_video_format_index = (id - ID_DYN_VIDEO_FORMAT) as usize;
                self.settings.dyn_video.format =
                    Format::from_index((id - ID_DYN_VIDEO_FORMAT) as usize);
                self.controls.load_dynamic(&self.settings, false);
                self.controls.read_ui(&mut self.settings);
                self.controls.refresh_formats();
            }
            ID_RESOLUTION
            | 163..=168
            | 173..=178
            | ID_SYSTEM_AUDIO
            | ID_MIC
            | ID_AUTOSTART..=ID_NOTIFY
            | ID_THREADS
            | ID_SCALE
            | ID_DYN_AUDIO_SYSTEM
            | ID_DYN_AUDIO_MIC => {
                self.controls.read_ui(&mut self.settings);
                self.sync_outline();
            }
            ID_SHOT_SCREEN | ID_TRAY_SHOT => self.screenshot_screen(),
            ID_SHOT_REGION => self.open_region(true, true),
            ID_TRAY_AREA => self.open_region(true, false),
            ID_CAPTURE_SCREEN => self.start_capture(self.settings.ui.profile(), None, false, true),
            ID_CAPTURE_REGION => self.open_region(false, true),
            ID_CAPTURE_AUDIO => self.start_capture(self.settings.ui.profile(), None, true, true),
            ID_TRAY_VIDEO => {
                self.start_capture(self.settings.dyn_video.profile(), None, false, false)
            }
            ID_TRAY_AUDIO => {
                let mut profile = self.settings.dyn_video.profile();
                profile.audio = self.settings.dyn_audio.clone();
                self.start_capture(profile, None, true, false);
            }
            ID_STOP | ID_TRAY_STOP => self.stop_capture(),
            ID_TRAY_OPEN => self.show_window(),
            ID_TRAY_EXIT => self.request_exit(),
            ID_STATUS if notification == 0 => self.open_saved(),
            id if ui::history_id(id) && notification == 0 => {
                let index = (id - ID_HISTORY) as usize;
                if let Some(path) = self.history.get(index).map(|(path, _)| path.clone()) {
                    self.open_path(&path);
                }
            }
            _ => {}
        }
    }

    fn screenshot_screen(&mut self) {
        self.controls.read_ui(&mut self.settings);
        let recording = self.mode == Mode::Recording;
        if recording {
            self.outline.hide();
        }
        let saved = if recording {
            media::save_display_screenshot(media::cursor_monitor_rect(), None)
        } else {
            media::save_screenshot(media::cursor_monitor_rect(), None)
        };
        match saved {
            Ok(path) => self.completed(path),
            Err(error) => self.error(&error),
        }
        if recording {
            self.sync_outline();
        }
    }

    fn open_region(&mut self, dynamic: bool, hide: bool) {
        if region::active() || self.mode == Mode::Saving || self.mode == Mode::Selecting {
            return;
        }
        if !dynamic && self.mode != Mode::Idle {
            return;
        }
        let recording = self.mode == Mode::Recording;
        self.hide_region = hide;
        self.controls.read_ui(&mut self.settings);
        if hide_while_active(hide, self.settings.hide_active) && !recording {
            self.restore_visible = unsafe { IsWindowVisible(self.hwnd) }.as_bool();
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
        if recording {
            self.outline.hide();
        }
        if !recording {
            self.mode = Mode::Selecting;
            self.update_state();
        }
        let profile = if dynamic {
            self.settings.dyn_area.profile()
        } else {
            self.settings.ui.profile()
        };
        if let Err(error) = region::open(profile, dynamic, !recording) {
            self.error(&error);
            if recording {
                self.sync_outline();
            } else {
                self.mode = Mode::Idle;
                self.restore_window();
                self.update_state();
            }
        }
    }

    fn region_done(&mut self) {
        let resume = self.mode == Mode::Recording || self.mode == Mode::Saving;
        match region::take_outcome() {
            Some(region::Outcome::Record(rect, profile)) => {
                if resume {
                    return;
                }
                self.mode = Mode::Idle;
                self.start_capture(profile, Some(rect), false, self.hide_region);
            }
            Some(region::Outcome::Saved(path)) => {
                if !resume {
                    self.mode = Mode::Idle;
                }
                self.completed(path);
                if self.mode == Mode::Idle {
                    self.restore_window();
                }
                self.update_state();
                if self.mode == Mode::Recording {
                    self.sync_outline();
                }
            }
            Some(region::Outcome::Cancel) | None => {
                if !resume {
                    self.mode = Mode::Idle;
                }
                if self.mode == Mode::Idle {
                    self.restore_window();
                }
                self.update_state();
                if self.mode == Mode::Recording {
                    self.sync_outline();
                }
            }
        }
    }

    fn start_capture(
        &mut self,
        profile: Profile,
        rect: Option<Rect>,
        audio_only: bool,
        hide: bool,
    ) {
        if self.mode != Mode::Idle || self.session.is_some() {
            return;
        }
        self.controls.read_ui(&mut self.settings);
        if audio_only && !profile.audio.system && !profile.audio.microphone {
            self.error("Enable System audio or Microphone before capturing audio");
            return;
        }
        if hide_while_active(hide, self.settings.hide_active) {
            self.restore_visible = unsafe { IsWindowVisible(self.hwnd) }.as_bool();
            unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        }
        self.record_rect = if audio_only {
            None
        } else {
            Some(rect.unwrap_or_else(media::cursor_monitor_rect))
        };
        let options = media::RecordOptions {
            profile,
            rect: self.record_rect,
            audio_only,
            threads: self.settings.threads,
        };
        match media::Session::start(options) {
            Ok(session) => {
                self.session = Some(session);
                self.started = Some(Instant::now());
                self.last_second = u64::MAX;
                self.mode = Mode::Recording;
                self.update_state();
                self.sync_outline();
                unsafe { SetTimer(self.hwnd, TIMER_ID, 200, None) };
            }
            Err(error) => {
                self.record_rect = None;
                self.outline.hide();
                self.error(&error);
                self.restore_window();
            }
        }
    }

    fn sync_outline(&mut self) {
        if region::active() {
            self.outline.hide();
            return;
        }
        let rect = if self.mode == Mode::Recording && self.settings.highlight_region {
            self.record_rect
        } else {
            None
        };
        let elapsed = self
            .started
            .map(|start| start.elapsed().as_millis())
            .unwrap_or(0);
        self.outline.sync(rect, elapsed);
    }

    fn stop_capture(&mut self) {
        if self.mode != Mode::Recording {
            return;
        }
        unsafe { KillTimer(self.hwnd, TIMER_ID) }.ok();
        self.record_rect = None;
        self.outline.hide();
        let Some(session) = self.session.take() else {
            return;
        };
        self.mode = Mode::Saving;
        self.saving_started = Some(Instant::now());
        self.update_state();
        match session.stop() {
            Ok(finalizer) => {
                self.finalizer = Some(finalizer);
                unsafe { SetTimer(self.hwnd, TIMER_ID, 100, None) };
            }
            Err(error) => self.finish_error(&error),
        }
    }

    fn tick(&mut self) {
        match self.mode {
            Mode::Recording => {
                let elapsed = self.started.map(|s| s.elapsed().as_secs()).unwrap_or(0);
                if elapsed != self.last_second {
                    self.last_second = elapsed;
                    self.controls.set_text(
                        ID_STOP,
                        &format!("Stop capture  {:02}:{:02}", elapsed / 60, elapsed % 60),
                    );
                    self.set_tray_status(&format!(
                        "ezcap — recording {:02}:{:02}",
                        elapsed / 60,
                        elapsed % 60
                    ));
                }
                self.sync_outline();
                if let Some(session) = &mut self.session {
                    if let Err(error) = session.tick() {
                        self.stop_capture();
                        self.finish_error(&error);
                    }
                }
            }
            Mode::Saving => {
                let elapsed = self
                    .saving_started
                    .map(|s| s.elapsed().as_secs())
                    .unwrap_or(0);
                self.controls.set_text(
                    ID_STOP,
                    &format!("Saving…  {:02}:{:02}", elapsed / 60, elapsed % 60),
                );
                let result = self.finalizer.as_mut().map(|f| f.step());
                match result {
                    Some(Ok(Some(path))) => {
                        self.finalizer = None;
                        self.mode = Mode::Idle;
                        self.saving_started = None;
                        self.completed(path);
                        if self.settings.completion_sound {
                            media::completion_sound();
                        }
                        self.restore_window();
                        self.update_state();
                        if self.quit_after_save {
                            self.request_exit();
                        }
                    }
                    Some(Ok(None)) => {}
                    Some(Err(error)) => self.finish_error(&error),
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn finish_error(&mut self, error: &str) {
        unsafe { KillTimer(self.hwnd, TIMER_ID) }.ok();
        self.session = None;
        self.finalizer = None;
        self.mode = Mode::Idle;
        self.saved_path = None;
        self.error(error);
        self.restore_window();
        self.update_state();
        if self.quit_after_save {
            self.request_exit();
        }
    }

    fn completed(&mut self, path: PathBuf) {
        if self.settings.clipboard {
            if let Err(error) = media::copy_file_to_clipboard(&path) {
                self.error(&error);
            }
        }
        self.controls
            .set_text(ID_STATUS, &format!("Saved: {}", path.display()));
        self.saved_path = Some(path.clone());
        self.history.insert(0, (path, history_line_now()));
        self.history.truncate(ui::HISTORY_COUNT as usize);
        let lines: Vec<String> = self
            .history
            .iter()
            .map(|(path, when)| {
                format!(
                    "{when}   {}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("file")
                )
            })
            .collect();
        self.controls.set_history(&lines);
    }

    fn open_saved(&mut self) {
        let Some(path) = self.saved_path.clone() else {
            return;
        };
        self.open_path(&path);
    }

    fn open_path(&mut self, path: &std::path::Path) {
        if let Err(error) = media::copy_text_to_clipboard(&path.display().to_string()) {
            self.error(&error);
        }
        if let Err(error) = media::reveal_in_folder(path) {
            self.error(&error);
        }
    }

    fn error(&self, text: &str) {
        self.controls.set_text(ID_STATUS, text);
        unsafe {
            MessageBoxW(
                self.hwnd,
                PCWSTR(wide(text).as_ptr()),
                TITLE,
                MB_OK | MB_ICONERROR,
            )
        };
    }

    fn restore_window(&mut self) {
        if self.restore_visible {
            self.restore_visible = false;
            self.show_window();
        }
    }

    fn show_window(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_RESTORE);
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
            SetFocus(self.hwnd);
        }
    }

    fn request_exit(&mut self) {
        if self.mode == Mode::Recording {
            self.quit_after_save = true;
            self.stop_capture();
            return;
        }
        if self.mode == Mode::Saving {
            self.quit_after_save = true;
            return;
        }
        if region::active() {
            region::cancel();
        }
        unsafe { Shell_NotifyIconW(NIM_DELETE, &self.tray) }.ok();
        if self.status_icon != self.base_icon {
            unsafe { DestroyIcon(self.status_icon) }.ok();
        }
        unsafe { DestroyIcon(self.base_icon) }.ok();
        unsafe { DestroyWindow(self.hwnd) }.ok();
    }

    fn update_state(&mut self) {
        let idle = self.mode == Mode::Idle;
        self.controls.set_idle(idle, self.mode == Mode::Recording);
        if self.mode == Mode::Idle {
            self.controls.set_text(ID_STOP, "Stop capture");
        }
        for id in [
            ID_TRAY_AREA,
            ID_TRAY_SHOT,
            ID_TRAY_VIDEO,
            ID_TRAY_AUDIO,
            ID_TRAY_STOP,
        ] {
            unsafe {
                EnableMenuItem(
                    self.tray_menu,
                    id as u32,
                    MF_BYCOMMAND
                        | if tray_item_enabled(self.mode, id) {
                            MF_ENABLED
                        } else {
                            MF_GRAYED
                        },
                )
            };
        }
        self.set_tray_status(match self.mode {
            Mode::Recording => "ezcap — recording",
            Mode::Saving => "ezcap — saving",
            _ => "ezcap",
        });
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CREATE => {
            APP = Box::into_raw(Box::new(App::new(hwnd)));
            app().build();
            LRESULT(0)
        }
        WM_COMMAND => {
            app().command(command_id(wparam.0), notification(wparam.0));
            LRESULT(0)
        }
        WM_HSCROLL => {
            let slider = HWND(lparam.0 as *mut std::ffi::c_void);
            let (fps_id, value_id) = if slider == app().controls.get(ID_DYN_AREA_FORMAT + 4) {
                (ID_DYN_AREA_FORMAT + 4, ID_DYN_AREA_FPS_VALUE)
            } else if slider == app().controls.get(ID_DYN_VIDEO_FORMAT + 4) {
                (ID_DYN_VIDEO_FORMAT + 4, ID_DYN_VIDEO_FPS_VALUE)
            } else {
                (ID_FPS, ID_FPS_VALUE)
            };
            let fps =
                SendMessageW(app().controls.get(fps_id), WM_USER, WPARAM(0), LPARAM(0)).0 as u32;
            app().controls.set_text(value_id, &format!("{} FPS", fps));
            app().controls.read_ui(&mut app().settings);
            LRESULT(0)
        }
        WM_DRAWITEM => {
            ui::draw_button(lparam, &app().controls);
            LRESULT(1)
        }
        WM_TIMER => {
            app().tick();
            LRESULT(0)
        }
        WM_TRAY => {
            match low(lparam.0 as usize) as u32 {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK | 1024 | 1025 => {
                    match geom::tray_click(app().mode.activity(), app().settings.tray_capture) {
                        geom::TrayClick::Stop => app().stop_capture(),
                        geom::TrayClick::OpenRegion => app().open_region(true, false),
                        geom::TrayClick::Show => app().show_window(),
                        geom::TrayClick::Ignore => {}
                    }
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => {
                    let mut point = POINT::default();
                    GetCursorPos(&mut point).ok();
                    SetForegroundWindow(hwnd);
                    TrackPopupMenu(
                        app().tray_menu,
                        TPM_RIGHTBUTTON,
                        point.x,
                        point.y,
                        0,
                        hwnd,
                        None,
                    );
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_REGION => {
            app().region_done();
            LRESULT(0)
        }
        WM_ACTIVATE_APP => {
            app().show_window();
            LRESULT(0)
        }
        WM_ERASEBKGND => {
            if APP.is_null() {
                return DefWindowProcW(hwnd, message, wparam, lparam);
            }
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).ok();
            FillRect(
                HDC(wparam.0 as *mut std::ffi::c_void),
                &rect,
                app().background,
            );
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC => {
            let dc = HDC(wparam.0 as *mut std::ffi::c_void);
            let control = HWND(lparam.0 as *mut std::ffi::c_void);
            let id = GetDlgCtrlID(control);
            let link = (id == ID_STATUS && app().saved_path.is_some()) || ui::history_id(id);
            SetTextColor(dc, if link { theme::ACCENT } else { TEXT });
            SetBkColor(dc, BACKGROUND);
            LRESULT(app().background.0 as isize)
        }
        WM_CLOSE => {
            if app().settings.close_tray {
                ShowWindow(hwnd, SW_HIDE);
            } else {
                app().request_exit();
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_ID).ok();
            app().outline.hide();
            region::shutdown();
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

enum MenuGlyph {
    Window,
    Area,
    Shot,
    Video,
    Audio,
    Stop,
    Exit,
}

fn menu_bitmap(glyph: MenuGlyph) -> HBITMAP {
    let size = 16i32;
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = size;
    info.bmiHeader.biHeight = -size;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB.0;
    let mut bits = std::ptr::null_mut();
    let screen = unsafe { GetDC(None) };
    let bitmap = unsafe {
        CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default()
    };
    unsafe { ReleaseDC(None, screen) };
    if bits.is_null() {
        return bitmap;
    }
    let pixels =
        unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), (size * size * 4) as usize) };
    let sea = theme::bgra(theme::ACCENT);
    let red = theme::bgra(theme::ALERT);
    let put = |pixels: &mut [u8], x: i32, y: i32, color: [u8; 4]| {
        if (0..size).contains(&x) && (0..size).contains(&y) {
            let index = ((y * size + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&color);
        }
    };
    match glyph {
        MenuGlyph::Window => {
            for x in 3..13 {
                put(pixels, x, 3, sea);
                put(pixels, x, 12, sea);
            }
            for y in 3..13 {
                put(pixels, 3, y, sea);
                put(pixels, 12, y, sea);
            }
            for x in 3..13 {
                put(pixels, x, 6, sea);
            }
        }
        MenuGlyph::Area => {
            for (x, y) in [
                (3, 3),
                (4, 3),
                (3, 4),
                (11, 3),
                (12, 3),
                (12, 4),
                (3, 11),
                (3, 12),
                (4, 12),
                (11, 12),
                (12, 11),
                (12, 12),
            ] {
                put(pixels, x, y, sea);
            }
        }
        MenuGlyph::Shot => {
            for x in 3..13 {
                put(pixels, x, 4, sea);
                put(pixels, x, 12, sea);
            }
            for y in 4..13 {
                put(pixels, 3, y, sea);
                put(pixels, 12, y, sea);
            }
            for (x, y) in [(7, 7), (8, 7), (7, 8), (8, 8)] {
                put(pixels, x, y, sea);
            }
        }
        MenuGlyph::Video => stamp_circle(pixels, size, 8, 8, 5, sea),
        MenuGlyph::Audio => {
            for y in 6..11 {
                put(pixels, 4, y, sea);
            }
            for y in 4..13 {
                put(pixels, 7, y, sea);
            }
            for y in 2..15 {
                put(pixels, 10, y, sea);
            }
        }
        MenuGlyph::Stop => {
            for y in 4..12 {
                for x in 4..12 {
                    put(pixels, x, y, red);
                }
            }
        }
        MenuGlyph::Exit => {
            for i in 0..8 {
                put(pixels, 4 + i, 4 + i, red);
                put(pixels, 11 - i, 4 + i, red);
            }
        }
    }
    bitmap
}

fn stamp_circle(pixels: &mut [u8], size: i32, cx: i32, cy: i32, radius: i32, color: [u8; 4]) {
    let radius_sq = radius * radius;
    for y in (cy - radius)..(cy + radius + 1) {
        for x in (cx - radius)..(cx + radius + 1) {
            if x < 0 || y < 0 || x >= size || y >= size {
                continue;
            }
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy > radius_sq {
                continue;
            }
            let index = ((y * size + x) * 4) as usize;
            pixels[index..index + 4].copy_from_slice(&color);
        }
    }
}

fn tray_status_icon(base: HICON, mode: Mode) -> HICON {
    if mode == Mode::Idle || mode == Mode::Selecting || base.is_invalid() {
        return base;
    }
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = 32;
        info.bmiHeader.biHeight = -32;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            DeleteDC(dc);
            ReleaseDC(None, screen);
            return base;
        };
        let old = SelectObject(dc, bitmap);
        let brush = CreateSolidBrush(BACKGROUND);
        FillRect(
            dc,
            &RECT {
                left: 0,
                top: 0,
                right: 32,
                bottom: 32,
            },
            brush,
        );
        DrawIconEx(dc, 0, 0, base, 32, 32, 0, None, DI_NORMAL).ok();
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(bits.cast::<u8>(), 32 * 32 * 4);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        let (fill, edge) = if mode == Mode::Recording {
            (
                theme::bgra(theme::BADGE_RECORDING),
                theme::bgra(theme::BADGE_RECORDING_EDGE),
            )
        } else {
            (
                theme::bgra(theme::BADGE_SAVING),
                theme::bgra(theme::BADGE_SAVING_EDGE),
            )
        };
        stamp_circle(pixels, 32, 23, 23, 8, edge);
        stamp_circle(pixels, 32, 23, 23, 6, fill);
        SelectObject(dc, old);
        DeleteObject(brush);
        let mask_bits = [0u8; 128];
        let mask = CreateBitmap(32, 32, 1, 1, Some(mask_bits.as_ptr().cast()));
        let icon_info = ICONINFO {
            fIcon: true.into(),
            hbmColor: bitmap,
            hbmMask: mask,
            ..Default::default()
        };
        let icon = CreateIconIndirect(&icon_info).unwrap_or(base);
        DeleteObject(bitmap);
        DeleteObject(mask);
        DeleteDC(dc);
        ReleaseDC(None, screen);
        icon
    }
}

fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).ok();
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
        let name = if test_mode() {
            w!("Local\\ezcap.test.single.instance")
        } else {
            w!("Local\\ezcap.single.instance")
        };
        let mutex = CreateMutexW(None, false, name).unwrap();
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(hwnd) = FindWindowW(CLASS, TITLE) {
                PostMessageW(hwnd, WM_ACTIVATE_APP, WPARAM(0), LPARAM(0)).ok();
            }
            return;
        }
        media::init();
        let instance = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            hIcon: LoadIconW(instance, module_icon()).unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut std::ffi::c_void),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassW(&class);
        region::register(instance.into());
        let minimized = std::env::args().any(|a| a == "--minimized");
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS,
            TITLE,
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            664,
            720,
            None,
            None,
            instance,
            None,
        )
        .unwrap();
        if !minimized {
            app().show_window();
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if !APP.is_null() {
            drop(Box::from_raw(APP));
            APP = std::ptr::null_mut();
        }
        CloseHandle(mutex).ok();
        windows::Win32::System::Com::CoUninitialize();
    }
}

#[cfg(test)]
mod ui_tests;
