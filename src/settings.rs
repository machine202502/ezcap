use crate::media::{self, AudioSettings, Format, VideoSettings};
use windows::core::{w, PCWSTR};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Registry::*;

pub(crate) fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn test_mode() -> bool {
    cfg!(test) || std::env::args().any(|arg| arg == "--test-mode")
}

#[derive(Clone)]
pub struct CaptureSettings {
    pub format: Format,
    pub video: [VideoSettings; 3],
    pub audio: AudioSettings,
}

impl CaptureSettings {
    pub fn new(format: Format) -> Self {
        Self {
            format,
            video: [
                VideoSettings {
                    height: 1080,
                    fps: 60,
                },
                VideoSettings {
                    height: 480,
                    fps: 30,
                },
                VideoSettings {
                    height: 480,
                    fps: 30,
                },
            ],
            audio: AudioSettings::default(),
        }
    }

    pub fn profile(&self) -> crate::media::Profile {
        crate::media::Profile {
            format: self.format,
            video: self.video[self.format.index()],
            audio: self.audio.clone(),
        }
    }
}

#[derive(Clone)]
pub struct Settings {
    pub ui: CaptureSettings,
    pub dyn_area: CaptureSettings,
    pub dyn_video: CaptureSettings,
    pub dyn_audio: AudioSettings,
    pub autostart: bool,
    pub start_minimized: bool,
    pub hide_active: bool,
    pub close_tray: bool,
    pub clipboard: bool,
    pub tray_capture: bool,
    pub highlight_region: bool,
    pub completion_sound: bool,
    pub threads: u32,
    pub scale: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui: CaptureSettings::new(Format::Mkv),
            dyn_area: CaptureSettings::new(Format::Gif),
            dyn_video: CaptureSettings::new(Format::Mkv),
            dyn_audio: AudioSettings::default(),
            autostart: false,
            start_minimized: false,
            hide_active: false,
            close_tray: false,
            clipboard: false,
            tray_capture: false,
            highlight_region: true,
            completion_sound: true,
            threads: 0,
            scale: 0,
        }
    }
}

struct Registry {
    key: HKEY,
}

impl Registry {
    fn open() -> Option<Self> {
        let mut key = HKEY::default();
        let path = if test_mode() {
            w!("Software\\ezcap-test")
        } else {
            w!("Software\\ezcap")
        };
        unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path,
                0,
                None,
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut key,
                None,
            )
        }
        .ok()
        .ok()?;
        Some(Self { key })
    }

    fn dword(&self, name: &str, default: u32) -> u32 {
        let name = wide(name);
        let mut value = default;
        let mut size = 4u32;
        unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(name.as_ptr()),
                None,
                None,
                Some((&mut value as *mut u32).cast()),
                Some(&mut size),
            )
        }
        .ok();
        value
    }

    fn set_dword(&self, name: &str, value: u32) {
        let name = wide(name);
        let bytes = value.to_le_bytes();
        unsafe { RegSetValueExW(self.key, PCWSTR(name.as_ptr()), 0, REG_DWORD, Some(&bytes)) }.ok();
    }

    fn string(&self, name: &str, default: &str) -> String {
        let name = wide(name);
        let mut size = 0u32;
        if unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(name.as_ptr()),
                None,
                None,
                None,
                Some(&mut size),
            )
        }
        .is_err()
            || size < 2
        {
            return default.into();
        }
        let mut data = vec![0u16; size as usize / 2];
        if unsafe {
            RegQueryValueExW(
                self.key,
                PCWSTR(name.as_ptr()),
                None,
                None,
                Some(data.as_mut_ptr().cast()),
                Some(&mut size),
            )
        }
        .is_err()
        {
            return default.into();
        }
        String::from_utf16_lossy(&data)
            .trim_end_matches('\0')
            .into()
    }

    fn set_string(&self, name: &str, value: &str) {
        let name = wide(name);
        let data = wide(value);
        let bytes =
            unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
        unsafe { RegSetValueExW(self.key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes)) }.ok();
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.key) }.ok();
    }
}

impl Settings {
    pub fn load() -> Self {
        let mut settings = Self::default();
        let Some(registry) = Registry::open() else {
            return settings;
        };
        settings.ui.format = Format::from_index(registry.dword("format", 0) as usize);
        for (index, name) in ["mkv", "gif", "webp"].iter().enumerate() {
            settings.ui.video[index].height =
                registry.dword(&format!("{name}_height"), settings.ui.video[index].height);
            settings.ui.video[index].fps = registry
                .dword(&format!("{name}_fps"), settings.ui.video[index].fps)
                .clamp(5, 60);
        }
        load_audio(&registry, "ui", &mut settings.ui.audio);
        load_profile(&registry, "area", &mut settings.dyn_area);
        load_profile(&registry, "video", &mut settings.dyn_video);
        load_audio(&registry, "audio", &mut settings.dyn_audio);
        settings.autostart = registry.dword("autostart", 0) != 0;
        settings.start_minimized = registry.dword("start_minimized", 0) != 0;
        settings.hide_active = registry.dword("hide_active", 0) != 0;
        settings.close_tray = registry.dword("close_tray", 0) != 0;
        settings.clipboard = registry.dword("clipboard", 0) != 0;
        settings.tray_capture = registry.dword("tray_capture", 0) != 0;
        settings.highlight_region = registry.dword("highlight_region", 1) != 0;
        settings.completion_sound = registry.dword("completion_sound", 1) != 0;
        settings.threads = registry.dword("threads", 0);
        settings.scale = registry.dword("scale", 0);
        for audio in [
            &mut settings.ui.audio,
            &mut settings.dyn_area.audio,
            &mut settings.dyn_video.audio,
            &mut settings.dyn_audio,
        ] {
            validate_audio_device(audio);
        }
        settings
    }

    pub fn save(&self) {
        let Some(registry) = Registry::open() else {
            return;
        };
        registry.set_dword("format", self.ui.format.index() as u32);
        for (index, name) in ["mkv", "gif", "webp"].iter().enumerate() {
            registry.set_dword(&format!("{name}_height"), self.ui.video[index].height);
            registry.set_dword(&format!("{name}_fps"), self.ui.video[index].fps);
        }
        save_audio(&registry, "ui", &self.ui.audio);
        save_profile(&registry, "area", &self.dyn_area);
        save_profile(&registry, "video", &self.dyn_video);
        save_audio(&registry, "audio", &self.dyn_audio);
        registry.set_dword("autostart", self.autostart as u32);
        registry.set_dword("start_minimized", self.start_minimized as u32);
        registry.set_dword("hide_active", self.hide_active as u32);
        registry.set_dword("close_tray", self.close_tray as u32);
        registry.set_dword("clipboard", self.clipboard as u32);
        registry.set_dword("tray_capture", self.tray_capture as u32);
        registry.set_dword("highlight_region", self.highlight_region as u32);
        registry.set_dword("completion_sound", self.completion_sound as u32);
        registry.set_dword("threads", self.threads);
        registry.set_dword("scale", self.scale);
        update_autostart(self.autostart, self.start_minimized);
    }
}

fn load_audio(registry: &Registry, prefix: &str, audio: &mut AudioSettings) {
    audio.system = registry.dword(&format!("{prefix}_system"), 1) != 0;
    audio.microphone = registry.dword(&format!("{prefix}_microphone"), 1) != 0;
    audio.device = registry.string(&format!("{prefix}_device"), "Auto");
}

fn save_audio(registry: &Registry, prefix: &str, audio: &AudioSettings) {
    registry.set_dword(&format!("{prefix}_system"), audio.system as u32);
    registry.set_dword(&format!("{prefix}_microphone"), audio.microphone as u32);
    registry.set_string(&format!("{prefix}_device"), &audio.device);
}

fn validate_audio_device(audio: &mut AudioSettings) {
    if audio.device != "Auto" && media::device_name(&audio.device).is_none() {
        audio.device = "Auto".into();
    }
}

fn load_profile(registry: &Registry, prefix: &str, profile: &mut CaptureSettings) {
    profile.format = Format::from_index(
        registry.dword(&format!("{prefix}_format"), profile.format.index() as u32) as usize,
    );
    for (index, name) in ["mkv", "gif", "webp"].iter().enumerate() {
        let legacy_height = if profile.format.index() == index {
            registry.dword(&format!("{prefix}_height"), profile.video[index].height)
        } else {
            profile.video[index].height
        };
        let legacy_fps = if profile.format.index() == index {
            registry.dword(&format!("{prefix}_fps"), profile.video[index].fps)
        } else {
            profile.video[index].fps
        };
        profile.video[index].height =
            registry.dword(&format!("{prefix}_{name}_height"), legacy_height);
        profile.video[index].fps = registry
            .dword(&format!("{prefix}_{name}_fps"), legacy_fps)
            .clamp(5, 60);
    }
    load_audio(registry, prefix, &mut profile.audio);
}

fn save_profile(registry: &Registry, prefix: &str, profile: &CaptureSettings) {
    registry.set_dword(&format!("{prefix}_format"), profile.format.index() as u32);
    for (index, name) in ["mkv", "gif", "webp"].iter().enumerate() {
        registry.set_dword(
            &format!("{prefix}_{name}_height"),
            profile.video[index].height,
        );
        registry.set_dword(&format!("{prefix}_{name}_fps"), profile.video[index].fps);
    }
    save_audio(registry, prefix, &profile.audio);
}

fn update_autostart(enabled: bool, minimized: bool) {
    if test_mode() {
        return;
    }
    let mut key = HKEY::default();
    if unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run"),
            0,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_READ | KEY_WRITE,
            None,
            &mut key,
            None,
        )
    }
    .is_err()
    {
        return;
    }
    if enabled {
        let mut path = vec![0u16; 32768];
        let len = unsafe { GetModuleFileNameW(None, &mut path) } as usize;
        let suffix = if minimized { " --minimized" } else { "" };
        let value = format!("\"{}\"{suffix}", String::from_utf16_lossy(&path[..len]));
        let data = wide(&value);
        let bytes =
            unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
        unsafe { RegSetValueExW(key, w!("ezcap"), 0, REG_SZ, Some(bytes)) }.ok();
    } else {
        unsafe { RegDeleteValueW(key, w!("ezcap")) }.ok();
    }
    unsafe { RegCloseKey(key) }.ok();
}
