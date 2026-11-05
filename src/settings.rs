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
    pub cursor: bool,
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
            cursor: true,
        }
    }

    pub fn profile(&self) -> crate::media::Profile {
        crate::media::Profile {
            format: self.format,
            video: self.video[self.format.index()],
            audio: self.audio.clone(),
            cursor: self.cursor,
        }
    }

    fn as_recording(&self) -> RecordingProfile {
        let picture = self.profile();
        RecordingProfile::Picture {
            format: picture.format,
            video: picture.video,
            audio: picture.audio,
            cursor: picture.cursor,
        }
    }
}

/// Which of the four saved profiles a recording starts from.
///
/// Capture, area, and video each remember a picture and the audio that goes with it.
/// Audio remembers only audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileRef {
    Capture,
    Area,
    Video,
    Audio,
}

impl ProfileRef {
    pub const ALL: [Self; 4] = [Self::Capture, Self::Area, Self::Video, Self::Audio];
}

/// What one recording is made of. An audio recording has no picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingProfile {
    Picture {
        format: Format,
        video: VideoSettings,
        audio: AudioSettings,
        cursor: bool,
    },
    Audio(AudioSettings),
}

impl RecordingProfile {
    pub fn audio(&self) -> &AudioSettings {
        match self {
            Self::Picture { audio, .. } | Self::Audio(audio) => audio,
        }
    }

    pub fn video(&self) -> Option<VideoSettings> {
        match self {
            Self::Picture { video, .. } => Some(*video),
            Self::Audio(_) => None,
        }
    }

    /// Drop the picture and keep this profile's audio. An audio profile stays as it is.
    pub fn without_picture(self) -> Self {
        match self {
            Self::Picture { audio, .. } | Self::Audio(audio) => Self::Audio(audio),
        }
    }

    /// Split into the encoder profile and whether the picture is recorded.
    /// The placeholder picture on an audio recording is not taken from any saved profile.
    pub fn session(self) -> (crate::media::Profile, bool) {
        match self {
            Self::Picture {
                format,
                video,
                audio,
                cursor,
            } => (
                crate::media::Profile {
                    format,
                    video,
                    audio,
                    cursor,
                },
                false,
            ),
            Self::Audio(audio) => (
                crate::media::Profile {
                    format: Format::Mkv,
                    video: VideoSettings { height: 0, fps: 30 },
                    audio,
                    cursor: false,
                },
                true,
            ),
        }
    }
}

#[derive(Clone)]
pub struct Settings {
    pub ui: CaptureSettings,
    pub tray_area: CaptureSettings,
    pub tray_video: CaptureSettings,
    pub tray_audio: AudioSettings,
    pub autostart: bool,
    pub start_minimized: bool,
    pub hide_active: bool,
    pub close_tray: bool,
    pub clipboard: bool,
    pub tray_capture: bool,
    pub highlight_region: bool,
    pub completion_sound: bool,
    pub show_no_signal: bool,
    pub show_card: bool,
    pub threads: u32,
    pub scale: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ui: CaptureSettings::new(Format::Mkv),
            tray_area: CaptureSettings::new(Format::Gif),
            tray_video: CaptureSettings::new(Format::Mkv),
            tray_audio: AudioSettings::default(),
            autostart: false,
            start_minimized: false,
            hide_active: false,
            close_tray: false,
            clipboard: false,
            tray_capture: false,
            highlight_region: true,
            completion_sound: true,
            show_no_signal: true,
            show_card: true,
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
            // SAFETY: The key is HKCU and the subkey is NUL-terminated. The returned HKEY is closed by its owner.

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
            // SAFETY: The key is open, the name is NUL-terminated, and the buffer length is the size passed in.

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
        unsafe {
            // SAFETY: The key is open, the name is NUL-terminated, and the buffer length is the size passed in.
            RegSetValueExW(self.key, PCWSTR(name.as_ptr()), 0, REG_DWORD, Some(&bytes))
        }
        .ok();
    }

    fn string(&self, name: &str, default: &str) -> String {
        let name = wide(name);
        let mut size = 0u32;
        if unsafe {
            // SAFETY: The key is open, the name is NUL-terminated, and the buffer length is the size passed in.

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
            // SAFETY: The pointer addresses a live buffer, and the length is exactly that buffer's size.

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
        let bytes = unsafe {
            // SAFETY: The pointer addresses a live buffer, and the length is exactly that buffer's size.
            std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2)
        };
        unsafe {
            // SAFETY: The key is open. The name and bytes outlive the call and match the declared size.
            RegSetValueExW(self.key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(bytes))
        }
        .ok();
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: This HKEY was opened above and is closed once.
            RegCloseKey(self.key)
        }
        .ok();
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
        settings.ui.cursor = registry.dword("ui_cursor", 1) != 0;
        load_profile(&registry, "area", &mut settings.tray_area);
        load_profile(&registry, "video", &mut settings.tray_video);
        load_audio(&registry, "audio", &mut settings.tray_audio);
        settings.autostart = registry.dword("autostart", 0) != 0;
        settings.start_minimized = registry.dword("start_minimized", 0) != 0;
        settings.hide_active = registry.dword("hide_active", 0) != 0;
        settings.close_tray = registry.dword("close_tray", 0) != 0;
        settings.clipboard = registry.dword("clipboard", 0) != 0;
        settings.tray_capture = registry.dword("tray_capture", 0) != 0;
        settings.highlight_region = registry.dword("highlight_region", 1) != 0;
        settings.completion_sound = registry.dword("completion_sound", 1) != 0;
        settings.show_no_signal = registry.dword("show_no_signal", 1) != 0;
        settings.show_card = registry.dword("show_card", 1) != 0;
        settings.threads = registry.dword("threads", 0);
        settings.scale = registry.dword("scale", 0);
        for source in ProfileRef::ALL {
            validate_audio_device(settings.audio_mut(source));
        }
        settings
    }

    pub fn recording(&self, source: ProfileRef) -> RecordingProfile {
        match source {
            ProfileRef::Capture => self.ui.as_recording(),
            ProfileRef::Area => self.tray_area.as_recording(),
            ProfileRef::Video => self.tray_video.as_recording(),
            ProfileRef::Audio => RecordingProfile::Audio(self.tray_audio.clone()),
        }
    }

    /// The profile a start button applies. An audio profile has no picture to keep,
    /// so asking it for one still returns audio only.
    pub fn applied(&self, source: ProfileRef, picture: bool) -> RecordingProfile {
        let profile = self.recording(source);
        if picture {
            profile
        } else {
            profile.without_picture()
        }
    }

    pub fn audio_mut(&mut self, source: ProfileRef) -> &mut AudioSettings {
        match source {
            ProfileRef::Capture => &mut self.ui.audio,
            ProfileRef::Area => &mut self.tray_area.audio,
            ProfileRef::Video => &mut self.tray_video.audio,
            ProfileRef::Audio => &mut self.tray_audio,
        }
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
        registry.set_dword("ui_cursor", self.ui.cursor as u32);
        save_profile(&registry, "area", &self.tray_area);
        save_profile(&registry, "video", &self.tray_video);
        save_audio(&registry, "audio", &self.tray_audio);
        registry.set_dword("autostart", self.autostart as u32);
        registry.set_dword("start_minimized", self.start_minimized as u32);
        registry.set_dword("hide_active", self.hide_active as u32);
        registry.set_dword("close_tray", self.close_tray as u32);
        registry.set_dword("clipboard", self.clipboard as u32);
        registry.set_dword("tray_capture", self.tray_capture as u32);
        registry.set_dword("highlight_region", self.highlight_region as u32);
        registry.set_dword("completion_sound", self.completion_sound as u32);
        registry.set_dword("show_no_signal", self.show_no_signal as u32);
        registry.set_dword("show_card", self.show_card as u32);
        registry.set_dword("threads", self.threads);
        registry.set_dword("scale", self.scale);
        update_autostart(self.autostart, self.start_minimized);
    }
}

fn load_audio(registry: &Registry, prefix: &str, audio: &mut AudioSettings) {
    audio.system = registry.dword(&format!("{prefix}_system"), 1) != 0;
    audio.microphone = registry.dword(&format!("{prefix}_microphone"), 1) != 0;
    audio.device = registry.string(&format!("{prefix}_device"), "Auto");
    audio.gain = registry
        .dword(&format!("{prefix}_gain"), audio.gain)
        .clamp(0, 300);
}

fn save_audio(registry: &Registry, prefix: &str, audio: &AudioSettings) {
    registry.set_dword(&format!("{prefix}_system"), audio.system as u32);
    registry.set_dword(&format!("{prefix}_microphone"), audio.microphone as u32);
    registry.set_string(&format!("{prefix}_device"), &audio.device);
    registry.set_dword(&format!("{prefix}_gain"), audio.gain);
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
    profile.cursor = registry.dword(&format!("{prefix}_cursor"), 1) != 0;
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
    registry.set_dword(&format!("{prefix}_cursor"), profile.cursor as u32);
}

fn update_autostart(enabled: bool, minimized: bool) {
    if test_mode() {
        return;
    }
    let mut key = HKEY::default();
    if unsafe {
        // SAFETY: The key is HKCU and the subkey is NUL-terminated. The returned HKEY is closed before this function returns.
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
        let len = unsafe {
            // SAFETY: The pointer addresses a live buffer, and the length is exactly that buffer's size.
            GetModuleFileNameW(None, &mut path)
        } as usize;
        let suffix = if minimized { " --minimized" } else { "" };
        let value = format!("\"{}\"{suffix}", String::from_utf16_lossy(&path[..len]));
        let data = wide(&value);
        let bytes = unsafe {
            // SAFETY: The pointer addresses a live buffer, and the length is exactly that buffer's size.
            std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2)
        };
        unsafe {
            // SAFETY: The key is open. The name and bytes outlive the call and match the declared size.
            RegSetValueExW(key, w!("ezcap"), 0, REG_SZ, Some(bytes))
        }
        .ok();
    } else {
        unsafe {
            // SAFETY: The key is open and the value name is a NUL-terminated literal.
            RegDeleteValueW(key, w!("ezcap"))
        }
        .ok();
    }
    unsafe {
        // SAFETY: This HKEY was opened above and is closed once.
        RegCloseKey(key)
    }
    .ok();
}

pub fn load_history(limit: usize) -> Vec<(std::path::PathBuf, String)> {
    let Some(registry) = Registry::open() else {
        return Vec::new();
    };
    decode_history(&registry.string("history", ""), limit)
}

pub fn save_history(items: &[(std::path::PathBuf, String)]) {
    let Some(registry) = Registry::open() else {
        return;
    };
    registry.set_string("history", &encode_history(items));
}

pub fn encode_history(items: &[(std::path::PathBuf, String)]) -> String {
    items
        .iter()
        .map(|(path, stamp)| format!("{stamp}\u{1f}{}", path.display()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn decode_history(text: &str, limit: usize) -> Vec<(std::path::PathBuf, String)> {
    text.lines()
        .filter_map(|line| {
            let (stamp, path) = line.split_once('\u{1f}')?;
            if stamp.is_empty() || path.is_empty() {
                return None;
            }
            Some((std::path::PathBuf::from(path), stamp.to_string()))
        })
        .take(limit)
        .collect()
}

#[cfg(test)]
mod recording_profiles {
    use super::{ProfileRef, Settings};
    use crate::media::{Format, VideoSettings};

    fn picture(height: u32, fps: u32) -> VideoSettings {
        VideoSettings { height, fps }
    }

    #[test]
    fn each_picture_profile_uses_its_own_format_and_microphone() {
        let mut settings = Settings::default();
        settings.ui.format = Format::Gif;
        settings.ui.video[Format::Gif.index()] = picture(720, 24);
        settings.ui.audio.gain = 110;
        settings.tray_area.format = Format::Webp;
        settings.tray_area.video[Format::Webp.index()] = picture(480, 15);
        settings.tray_area.audio.gain = 120;
        settings.tray_video.format = Format::Mkv;
        settings.tray_video.video[Format::Mkv.index()] = picture(1080, 60);
        settings.tray_video.audio.gain = 130;
        settings.tray_audio.gain = 140;

        let capture = settings.applied(ProfileRef::Capture, true);
        let (capture_session, capture_audio_only) = capture.clone().session();
        assert!(!capture_audio_only);
        assert_eq!(capture_session.format, Format::Gif);
        assert_eq!(capture.video(), Some(picture(720, 24)));
        assert_eq!(capture.audio().gain, 110);
        assert!(capture_session.cursor);
        settings.ui.cursor = false;
        settings.tray_area.cursor = true;
        let capture_picture = settings.applied(ProfileRef::Capture, true).session().0;
        let area_picture = settings.recording(ProfileRef::Area).session().0;
        assert!(!capture_picture.cursor);
        assert!(area_picture.cursor);

        let capture_audio = settings.applied(ProfileRef::Capture, false);
        let (_, sound_only) = capture_audio.clone().session();
        assert!(sound_only);
        assert_eq!(capture_audio.video(), None);
        assert_eq!(capture_audio.audio().gain, 110);

        let area = settings.recording(ProfileRef::Area);
        let (area_session, area_audio_only) = area.clone().session();
        assert!(!area_audio_only);
        assert_eq!(area_session.format, Format::Webp);
        assert_eq!(area.video(), Some(picture(480, 15)));
        assert_eq!(area.audio().gain, 120);

        let video = settings.recording(ProfileRef::Video);
        let (video_session, video_audio_only) = video.clone().session();
        assert!(!video_audio_only);
        assert_eq!(video.video(), Some(picture(1080, 60)));
        assert_eq!(video_session.video, picture(1080, 60));
        assert_eq!(video.audio().gain, 130);

        let audio = settings.applied(ProfileRef::Audio, true);
        let (_, audio_only) = audio.clone().session();
        assert!(audio_only);
        assert_eq!(audio.video(), None);
        assert_eq!(audio.audio().gain, 140);
    }

    #[test]
    fn every_picture_profile_keeps_its_own_pointer_for_recording() {
        let mut settings = Settings::default();
        settings.ui.cursor = true;
        settings.tray_area.cursor = false;
        settings.tray_video.cursor = true;

        let capture = settings.applied(ProfileRef::Capture, true).session().0;
        let area = settings.applied(ProfileRef::Area, true).session().0;
        let video = settings.applied(ProfileRef::Video, true).session().0;
        assert!(capture.cursor);
        assert!(!area.cursor);
        assert!(video.cursor);
    }

    #[test]
    fn an_audio_recording_does_not_borrow_another_profiles_picture() {
        let mut tall = Settings::default();
        tall.tray_video.video[Format::Mkv.index()] = picture(2160, 60);
        tall.tray_audio.system = false;
        tall.tray_audio.gain = 180;
        let mut small = Settings::default();
        small.tray_video.video[Format::Mkv.index()] = picture(360, 15);
        small.tray_audio.system = false;
        small.tray_audio.gain = 180;

        let (tall_session, tall_audio_only) = tall.applied(ProfileRef::Audio, true).session();
        let (small_session, small_audio_only) = small.applied(ProfileRef::Audio, false).session();
        assert!(tall_audio_only && small_audio_only);
        assert_eq!(tall_session.video, small_session.video);
        assert_ne!(tall_session.video.height, 2160);
        assert_eq!(tall_session.audio.gain, 180);
        assert!(!tall_session.audio.system);

        let (video, audio_only) = tall.applied(ProfileRef::Video, true).session();
        assert!(!audio_only);
        assert_eq!(video.video, picture(2160, 60));
        assert_ne!(video.audio.gain, 180);
    }
}

#[cfg(test)]
mod history_store {
    use super::{decode_history, encode_history};
    use std::path::PathBuf;

    #[test]
    fn the_visible_history_survives_a_restart() {
        let items = vec![
            (
                PathBuf::from(r"C:\Pictures\ezcap\Video_1.mkv"),
                "4 Nov 2026, 22:56:07".into(),
            ),
            (
                PathBuf::from(r"C:\Pictures\ezcap\Shot 2.png"),
                "4 Nov 2026, 22:01:00".into(),
            ),
        ];
        let restored = decode_history(&encode_history(&items), 12);
        assert_eq!(restored, items);
        let extra: Vec<_> = (0..20)
            .map(|index| {
                (
                    PathBuf::from(format!("file-{index}.png")),
                    format!("stamp-{index}"),
                )
            })
            .collect();
        assert_eq!(decode_history(&encode_history(&extra), 12).len(), 12);
    }
}
