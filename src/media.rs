use ffmpeg::{
    codec, encoder, format, frame, packet::Mut as _, software::scaling, util::format::Pixel,
    Dictionary, Packet, Rational,
};
use ffmpeg_next as ffmpeg;
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Format {
    Mkv,
    Gif,
    Webp,
}

impl Format {
    pub fn index(self) -> usize {
        match self {
            Self::Mkv => 0,
            Self::Gif => 1,
            Self::Webp => 2,
        }
    }

    pub fn from_index(index: usize) -> Self {
        match index {
            1 => Self::Gif,
            2 => Self::Webp,
            _ => Self::Mkv,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mkv => "mkv",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }
}

#[derive(Clone, Copy)]
pub struct VideoSettings {
    pub height: u32,
    pub fps: u32,
}

#[derive(Clone)]
pub struct AudioSettings {
    pub system: bool,
    pub microphone: bool,
    pub device: String,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            system: true,
            microphone: true,
            device: "Auto".into(),
        }
    }
}

#[derive(Clone)]
pub struct Profile {
    pub format: Format,
    pub video: VideoSettings,
    pub audio: AudioSettings,
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

pub struct RecordOptions {
    pub profile: Profile,
    pub rect: Option<Rect>,
    pub audio_only: bool,
    pub threads: u32,
    /// When set, the staged window is captured instead of `rect`.
    pub window: bool,
    /// Keep recording if the chosen monitor or window disappears.
    pub resilient: bool,
    pub show_no_signal: bool,
    /// When the signal is gone and recording continues, show the card instead of the last frame.
    pub show_card: bool,
}

#[derive(Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

pub fn init() {
    let _ = ffmpeg::init();
}

pub fn cursor_monitor_rect() -> Rect {
    // SAFETY: The POINT and MONITORINFO are writable locals. cbSize is set before GetMonitorInfoW.
    unsafe {
        let mut point = POINT::default();
        GetCursorPos(&mut point).ok();
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok();
        let r = info.rcMonitor;
        Rect {
            x: r.left,
            y: r.top,
            w: (r.right - r.left) as u32,
            h: (r.bottom - r.top) as u32,
        }
    }
}

pub fn grab(rect: Rect) -> Result<Frame, String> {
    // SAFETY: The screen DC is released before return. The bitmap header size is set, and the copy uses only the requested byte count.
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return Err("GetDC failed".into());
        }
        let memory = CreateCompatibleDC(screen);
        let mut bitmap_info: BITMAPINFO = std::mem::zeroed();
        bitmap_info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bitmap_info.bmiHeader.biWidth = rect.w as i32;
        bitmap_info.bmiHeader.biHeight = -(rect.h as i32);
        bitmap_info.bmiHeader.biPlanes = 1;
        bitmap_info.bmiHeader.biBitCount = 32;
        bitmap_info.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(screen, &bitmap_info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| e.to_string())?;
        let old = SelectObject(memory, bitmap);
        BitBlt(
            memory,
            0,
            0,
            rect.w as i32,
            rect.h as i32,
            screen,
            rect.x,
            rect.y,
            SRCCOPY | CAPTUREBLT,
        )
        .map_err(|e| e.to_string())?;
        let len = rect.w as usize * rect.h as usize * 4;
        let mut data = vec![0u8; len];
        std::ptr::copy_nonoverlapping(bits.cast::<u8>(), data.as_mut_ptr(), len);
        let mut cursor = CURSORINFO {
            cbSize: std::mem::size_of::<CURSORINFO>() as u32,
            ..Default::default()
        };
        if GetCursorInfo(&mut cursor).is_ok() && cursor.flags == CURSOR_SHOWING {
            let mut icon_info = ICONINFO::default();
            if GetIconInfo(cursor.hCursor, &mut icon_info).is_ok() {
                let x = cursor.ptScreenPos.x - rect.x - icon_info.xHotspot as i32;
                let y = cursor.ptScreenPos.y - rect.y - icon_info.yHotspot as i32;
                DrawIconEx(memory, x, y, cursor.hCursor, 0, 0, 0, None, DI_NORMAL).ok();
                if !icon_info.hbmColor.is_invalid() {
                    DeleteObject(icon_info.hbmColor);
                }
                if !icon_info.hbmMask.is_invalid() {
                    DeleteObject(icon_info.hbmMask);
                }
                std::ptr::copy_nonoverlapping(bits.cast::<u8>(), data.as_mut_ptr(), len);
            }
        }
        SelectObject(memory, old);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(None, screen);
        Ok(Frame {
            width: rect.w,
            height: rect.h,
            data,
        })
    }
}

pub fn output_path(kind: &str, extension: &str) -> Result<PathBuf, String> {
    // SAFETY: The folder path from the shell is freed with CoTaskMemFree before this function returns. Only the characters up to the NUL are read.
    unsafe {
        let raw = SHGetKnownFolderPath(&FOLDERID_Pictures, KNOWN_FOLDER_FLAG(0), None)
            .map_err(|e| e.to_string())?;
        let mut len = 0;
        while *raw.0.add(len) != 0 {
            len += 1;
        }
        let root = String::from_utf16_lossy(std::slice::from_raw_parts(raw.0, len));
        CoTaskMemFree(Some(raw.0.cast()));
        let dir = PathBuf::from(root).join("ezcap");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let time = windows::Win32::System::SystemInformation::GetLocalTime();
        for counter in 1..10000 {
            let suffix = if counter == 1 {
                String::new()
            } else {
                format!("-{counter}")
            };
            let name = format!(
                "{kind}_{:04}-{:02}-{:02}_{:02}-{:02}-{:02}{suffix}.{extension}",
                time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
            );
            let path = dir.join(name);
            if !path.exists() {
                return Ok(path);
            }
        }
        Err("Cannot allocate an output filename".into())
    }
}

pub fn save_screenshot(rect: Rect, path: Option<PathBuf>) -> Result<PathBuf, String> {
    write_screenshot(crate::capture::grab_still(rect)?, path)
}

pub fn save_display_screenshot(rect: Rect, path: Option<PathBuf>) -> Result<PathBuf, String> {
    write_screenshot(crate::capture::grab_display(rect)?, path)
}

fn write_screenshot(frame: Frame, path: Option<PathBuf>) -> Result<PathBuf, String> {
    let path = match path {
        Some(path) => path,
        None => output_path("Image", "png")?,
    };
    save_frame(&frame, &path)?;
    Ok(path)
}

pub fn save_frame(frame_data: &Frame, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let webp = extension == "webp";
    let codec = if webp {
        encoder::find_by_name("libwebp").or_else(|| encoder::find(codec::Id::WEBP))
    } else {
        encoder::find(codec::Id::PNG)
    }
    .ok_or("Image encoder is unavailable")?;
    let mut output = format::output(path).map_err(|e| e.to_string())?;
    let mut stream = output.add_stream(codec).map_err(|e| e.to_string())?;
    stream.set_time_base(Rational(1, 1));
    let mut context = codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()
        .map_err(|e| e.to_string())?;
    context.set_width(frame_data.width);
    context.set_height(frame_data.height);
    context.set_format(if webp { Pixel::YUV420P } else { Pixel::RGBA });
    context.set_time_base(Rational(1, 1));
    let mut options = Dictionary::new();
    if webp {
        options.set("lossless", "1");
        options.set("quality", "100");
    }
    let mut encoder = context.open_with(options).map_err(|e| e.to_string())?;
    stream.set_parameters(&encoder);
    let mut mux_options = Dictionary::new();
    mux_options.set("update", "1");
    output
        .write_header_with(mux_options)
        .map_err(|e| e.to_string())?;
    let mut source = frame::Video::new(Pixel::BGRA, frame_data.width, frame_data.height);
    copy_bgra(frame_data, &mut source)?;
    let mut converted = frame::Video::empty();
    if encoder.format() == Pixel::BGRA {
        converted = source;
    } else {
        let mut scaler = scaling::Context::get(
            Pixel::BGRA,
            frame_data.width,
            frame_data.height,
            encoder.format(),
            frame_data.width,
            frame_data.height,
            scaling::flag::Flags::BICUBIC,
        )
        .map_err(|e| e.to_string())?;
        scaler
            .run(&source, &mut converted)
            .map_err(|e| e.to_string())?;
    }
    converted.set_pts(Some(0));
    encoder.send_frame(&converted).map_err(|e| e.to_string())?;
    write_image_packets(&mut encoder, &mut output)?;
    encoder.send_eof().map_err(|e| e.to_string())?;
    write_image_packets(&mut encoder, &mut output)?;
    output.write_trailer().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
pub fn read_image(path: &Path) -> Result<Frame, String> {
    let mut input = format::input(path).map_err(|e| e.to_string())?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or("Missing image stream")?;
    let index = stream.index();
    let mut decoder = codec::context::Context::from_parameters(stream.parameters())
        .map_err(|e| e.to_string())?
        .decoder()
        .video()
        .map_err(|e| e.to_string())?;
    for (stream, packet) in input.packets() {
        if stream.index() != index {
            continue;
        }
        decoder.send_packet(&packet).map_err(|e| e.to_string())?;
        let mut decoded = frame::Video::empty();
        if decoder.receive_frame(&mut decoded).is_ok() {
            let mut converted = frame::Video::empty();
            let mut scaler = scaling::Context::get(
                decoded.format(),
                decoded.width(),
                decoded.height(),
                Pixel::BGRA,
                decoded.width(),
                decoded.height(),
                scaling::flag::Flags::BILINEAR,
            )
            .map_err(|e| e.to_string())?;
            scaler
                .run(&decoded, &mut converted)
                .map_err(|e| e.to_string())?;
            let width = converted.width();
            let height = converted.height();
            let line = width as usize * 4;
            let stride = converted.stride(0);
            let plane = converted.data(0);
            let mut data = vec![0u8; line * height as usize];
            for y in 0..height as usize {
                data[y * line..(y + 1) * line]
                    .copy_from_slice(&plane[y * stride..y * stride + line]);
            }
            return Ok(Frame {
                width,
                height,
                data,
            });
        }
    }
    Err("Cannot decode image".into())
}

fn write_image_packets(
    encoder: &mut encoder::video::Encoder,
    output: &mut format::context::Output,
) -> Result<(), String> {
    let mut packet = Packet::empty();
    while encoder.receive_packet(&mut packet).is_ok() {
        packet.set_stream(0);
        packet
            .write_interleaved(output)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn copy_bgra(data: &Frame, destination: &mut frame::Video) -> Result<(), String> {
    let line = data.width as usize * 4;
    if data.data.len() != line * data.height as usize
        || destination.width() != data.width
        || destination.height() != data.height
    {
        return Err("Frame size does not match the recording".into());
    }
    let stride = destination.stride(0);
    let plane = destination.data_mut(0);
    if stride < line || plane.len() < stride * data.height as usize {
        return Err("Frame size does not match the recording".into());
    }
    for y in 0..data.height as usize {
        plane[y * stride..y * stride + line].copy_from_slice(&data.data[y * line..(y + 1) * line]);
    }
    Ok(())
}

pub fn copy_text_to_clipboard(text: &str) -> Result<(), String> {
    // SAFETY: The clipboard is opened and closed on this thread. The allocated block matches the UTF-16 text and is given to the clipboard.
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        EmptyClipboard().map_err(|e| e.to_string())?;
        let wide = text
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let bytes = wide.len() * 2;
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| e.to_string())?;
        let pointer = GlobalLock(memory) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer, wide.len());
        GlobalUnlock(memory).ok();
        SetClipboardData(13, HANDLE(memory.0)).map_err(|e| e.to_string())?;
        CloseClipboard().ok();
        Ok(())
    }
}

pub fn open_file(path: &Path) -> Result<(), String> {
    let verb = "open"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let file = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: The verb and path are NUL-terminated and live until ShellExecuteW returns. The verb open uses the file's associated program.
    let code = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
        .0 as isize
    };
    if code > 32 {
        Ok(())
    } else {
        Err("Cannot open the file".into())
    }
}

pub fn reveal_in_folder(path: &Path) -> Result<(), String> {
    let verb = "open"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let program = "explorer.exe"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let params = format!("/select,\"{}\"", path.display())
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: The verb, program, and parameters are NUL-terminated and live until ShellExecuteW returns.
    let code = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(program.as_ptr()),
            PCWSTR(params.as_ptr()),
            None,
            SW_SHOWNORMAL,
        )
        .0 as isize
    };
    if code > 32 {
        Ok(())
    } else {
        Err("Cannot open the file's folder".into())
    }
}

pub fn copy_file_to_clipboard(path: &Path) -> Result<(), String> {
    // SAFETY: The clipboard is opened and closed on this thread. The drop list is sized to the path and given to the clipboard.
    unsafe {
        OpenClipboard(None).map_err(|e| e.to_string())?;
        EmptyClipboard().map_err(|e| e.to_string())?;
        let path = path
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let bytes = std::mem::size_of::<DROPFILES>() + (path.len() + 1) * 2;
        let memory = GlobalAlloc(GMEM_MOVEABLE, bytes).map_err(|e| e.to_string())?;
        let pointer = GlobalLock(memory) as *mut u8;
        let drop = DROPFILES {
            pFiles: std::mem::size_of::<DROPFILES>() as u32,
            fWide: true.into(),
            ..Default::default()
        };
        std::ptr::write(pointer.cast::<DROPFILES>(), drop);
        std::ptr::copy_nonoverlapping(
            path.as_ptr(),
            pointer.add(std::mem::size_of::<DROPFILES>()).cast::<u16>(),
            path.len(),
        );
        std::ptr::write(pointer.add(bytes - 2), 0);
        GlobalUnlock(memory).ok();
        SetClipboardData(CF_HDROP.0 as u32, HANDLE(memory.0)).map_err(|e| e.to_string())?;
        CloseClipboard().ok();
        Ok(())
    }
}

pub fn copy_image_to_clipboard(frame: &Frame) -> Result<(), String> {
    // SAFETY: The screen DC is released before return. The bitmap header size is set, and the copy uses only the frame's byte count.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = frame.width as i32;
        info.bmiHeader.biHeight = -(frame.height as i32);
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| e.to_string())?;
        std::ptr::copy_nonoverlapping(frame.data.as_ptr(), bits.cast::<u8>(), frame.data.len());
        SelectObject(dc, bitmap);
        OpenClipboard(None).map_err(|e| e.to_string())?;
        EmptyClipboard().map_err(|e| e.to_string())?;
        SetClipboardData(
            windows::Win32::System::Ole::CF_BITMAP.0 as u32,
            HANDLE(bitmap.0),
        )
        .map_err(|e| e.to_string())?;
        CloseClipboard().ok();
        DeleteDC(dc);
        ReleaseDC(None, screen);
        Ok(())
    }
}

pub fn completion_sound() {
    static SOUND: &[u8] = include_bytes!("../assets/recording-saved.wav");
    // SAFETY: The WAV bytes are embedded for the process lifetime, and SND_MEMORY plays that buffer.
    unsafe {
        windows::Win32::Media::Audio::PlaySoundW(
            PCWSTR(SOUND.as_ptr().cast()),
            None,
            windows::Win32::Media::Audio::SND_MEMORY
                | windows::Win32::Media::Audio::SND_ASYNC
                | windows::Win32::Media::Audio::SND_NODEFAULT,
        )
        .ok();
    }
}

fn device_enumerator() -> Result<windows::Win32::Media::Audio::IMMDeviceEnumerator, String> {
    // SAFETY: The COM object is released when this Result is dropped.
    unsafe {
        windows::Win32::System::Com::CoCreateInstance(
            &windows::Win32::Media::Audio::MMDeviceEnumerator,
            None,
            windows::Win32::System::Com::CLSCTX_ALL,
        )
        .map_err(|e| e.to_string())
    }
}

fn pwstr(value: windows::core::PWSTR) -> String {
    // SAFETY: A null pointer returns before any read. Otherwise only the characters up to the NUL are copied.
    unsafe {
        if value.is_null() {
            return String::new();
        }
        let mut len = 0;
        while *value.0.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(value.0, len));
        CoTaskMemFree(Some(value.0.cast()));
        text
    }
}

fn endpoint_name(device: &windows::Win32::Media::Audio::IMMDevice) -> String {
    // SAFETY: The device is alive. The allocated friendly name is freed with CoTaskMemFree before return.
    unsafe {
        use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
        use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
        let Ok(store) = device.OpenPropertyStore(windows::Win32::System::Com::STGM_READ) else {
            return "Microphone".into();
        };
        let Ok(value) = store.GetValue(&PKEY_Device_FriendlyName) else {
            return "Microphone".into();
        };
        let Ok(text) = PropVariantToStringAlloc(&value) else {
            return "Microphone".into();
        };
        pwstr(text)
    }
}

pub fn microphones() -> Vec<AudioDevice> {
    (|| {
        let enumerator = device_enumerator().ok()?;
        // SAFETY: The enumerator is alive. The returned collection is released when dropped.
        let collection = unsafe {
            enumerator.EnumAudioEndpoints(
                windows::Win32::Media::Audio::eCapture,
                windows::Win32::Media::Audio::DEVICE_STATE_ACTIVE,
            )
        }
        .ok()?;
        // SAFETY: The endpoint collection is alive for this call.
        let count = unsafe { collection.GetCount() }.ok()?;
        let mut result = Vec::new();
        for index in 0..count {
            // SAFETY: index is below the count this collection just returned.
            let Ok(device) = (unsafe { collection.Item(index) }) else {
                continue;
            };
            // SAFETY: The device came from this live collection.
            let Ok(id) = (unsafe { device.GetId() }) else {
                continue;
            };
            result.push(AudioDevice {
                id: pwstr(id),
                name: endpoint_name(&device),
            });
        }
        Some(result)
    })()
    .unwrap_or_default()
}

pub fn device_at(index: usize) -> Option<AudioDevice> {
    microphones().get(index).cloned()
}

pub fn device_name(id: &str) -> Option<String> {
    microphones()
        .into_iter()
        .find(|d| d.id == id)
        .map(|d| d.name)
}

pub(crate) struct Written {
    pub audio_packets: u64,
    pub video_packets: u64,
}

pub(crate) struct VideoWriter {
    output: format::context::Output,
    encoder: Option<encoder::video::Encoder>,
    scaler: Option<scaling::Context>,
    source: frame::Video,
    converted: frame::Video,
    audio: Vec<AudioTrack>,
    next_pts: i64,
    video_packets: u64,
    encoder_tb: Rational,
    stream_tb: Rational,
    closed: bool,
}

struct AudioTrack {
    encoder: encoder::audio::Encoder,
    samples: Vec<f32>,
    next_pts: i64,
    packets: u64,
    encoder_tb: Rational,
    stream_tb: Rational,
    stream_index: usize,
    frame_size: usize,
    mp3: bool,
}

impl AudioTrack {
    fn new(output: &mut format::context::Output, mp3: bool) -> Result<Self, String> {
        let codec = if mp3 {
            encoder::find_by_name("libmp3lame")
        } else {
            encoder::find(codec::Id::AAC)
        }
        .ok_or("Audio encoder is unavailable")?;
        let stream_index = output.nb_streams() as usize;
        let mut stream = output.add_stream(codec).map_err(|e| e.to_string())?;
        stream.set_time_base(Rational(1, 48000));
        let mut context = codec::context::Context::new_with_codec(codec)
            .encoder()
            .audio()
            .map_err(|e| e.to_string())?;
        context.set_rate(48000);
        context.set_channel_layout(ffmpeg::ChannelLayout::STEREO);
        context.set_format(if mp3 {
            ffmpeg::util::format::Sample::I16(ffmpeg::util::format::sample::Type::Planar)
        } else {
            ffmpeg::util::format::Sample::F32(ffmpeg::util::format::sample::Type::Planar)
        });
        context.set_bit_rate(if mp3 { 320000 } else { 192000 });
        context.set_time_base(Rational(1, 48000));
        let encoder = context.open().map_err(|e| e.to_string())?;
        stream.set_parameters(&encoder);
        let encoder_tb = encoder.time_base();
        let reported = encoder.frame_size() as usize;
        let frame_size = if reported == 0 {
            if mp3 {
                1152
            } else {
                1024
            }
        } else {
            reported
        };
        Ok(Self {
            encoder,
            samples: Vec::new(),
            next_pts: 0,
            packets: 0,
            encoder_tb,
            stream_tb: Rational(1, 48000),
            stream_index,
            frame_size,
            mp3,
        })
    }

    fn pending_end(&self) -> u64 {
        self.next_pts as u64 + (self.samples.len() / 2) as u64
    }

    fn push_from(
        &mut self,
        start_sample: u64,
        input: &[f32],
        output: &mut format::context::Output,
    ) -> Result<(), String> {
        let input = &input[..input.len() & !1];
        if input.is_empty() && self.samples.len() < self.frame_size * 2 {
            return Ok(());
        }
        let end = self.pending_end();
        if start_sample < end {
            let skip = ((end - start_sample) as usize).saturating_mul(2);
            if skip >= input.len() {
                return Ok(());
            }
            self.samples.extend_from_slice(&input[skip..]);
        } else {
            let gap = ((start_sample - end) as usize).saturating_mul(2);
            self.samples.extend(std::iter::repeat_n(0.0, gap));
            self.samples.extend_from_slice(input);
        }
        self.encode_ready(output)
    }

    fn encode_ready(&mut self, output: &mut format::context::Output) -> Result<(), String> {
        let needed = self.frame_size * 2;
        while self.samples.len() >= needed {
            let sample_format = if self.mp3 {
                ffmpeg::util::format::Sample::I16(ffmpeg::util::format::sample::Type::Planar)
            } else {
                ffmpeg::util::format::Sample::F32(ffmpeg::util::format::sample::Type::Planar)
            };
            let mut audio_frame = frame::Audio::new(
                sample_format,
                self.frame_size,
                ffmpeg::ChannelLayout::STEREO,
            );
            audio_frame.set_rate(48000);
            if self.mp3 {
                for (index, target) in audio_frame.plane_mut::<i16>(0).iter_mut().enumerate() {
                    *target = (self.samples[index * 2].clamp(-1.0, 1.0) * 32767.0) as i16;
                }
                for (index, target) in audio_frame.plane_mut::<i16>(1).iter_mut().enumerate() {
                    *target = (self.samples[index * 2 + 1].clamp(-1.0, 1.0) * 32767.0) as i16;
                }
            } else {
                for (index, target) in audio_frame.plane_mut::<f32>(0).iter_mut().enumerate() {
                    *target = self.samples[index * 2].clamp(-1.0, 1.0);
                }
                for (index, target) in audio_frame.plane_mut::<f32>(1).iter_mut().enumerate() {
                    *target = self.samples[index * 2 + 1].clamp(-1.0, 1.0);
                }
            }
            audio_frame.set_pts(Some(self.next_pts));
            self.next_pts += self.frame_size as i64;
            self.encoder
                .send_frame(&audio_frame)
                .map_err(|e| e.to_string())?;
            self.drain(output)?;
            self.samples.drain(..needed);
        }
        Ok(())
    }

    fn drain(&mut self, output: &mut format::context::Output) -> Result<(), String> {
        let mut packet = Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(self.stream_index);
            packet.rescale_ts(self.encoder_tb, self.stream_tb);
            packet
                .write_interleaved(output)
                .map_err(|e| e.to_string())?;
            self.packets += 1;
        }
        Ok(())
    }

    fn finish(&mut self, output: &mut format::context::Output) -> Result<(), String> {
        if !self.samples.is_empty() {
            let needed = self.frame_size * 2;
            self.samples.resize(needed, 0.0);
            self.encode_ready(output)?;
        }
        self.encoder.send_eof().map_err(|e| e.to_string())?;
        self.drain(output)
    }
}

fn scaled_size(width: u32, height: u32, limit: u32) -> (u32, u32) {
    if height <= limit || limit == 0 {
        return (width & !1, height & !1);
    }
    (
        ((width as u64 * limit as u64 / height as u64) as u32) & !1,
        limit & !1,
    )
}

impl VideoWriter {
    pub(crate) fn new(
        path: &Path,
        rect: Rect,
        settings: VideoSettings,
        format_kind: Format,
        threads: u32,
        video: bool,
        audio: bool,
    ) -> Result<Self, String> {
        let (width, height) = scaled_size(rect.w, rect.h, settings.height);
        let animation = format_kind != Format::Mkv;
        let mp3 = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mp3"));
        let mut output = format::output(path).map_err(|e| e.to_string())?;
        let mut encoder = None;
        if video {
            let codec = encoder::find_by_name(if animation { "libx264rgb" } else { "libx264" })
                .ok_or("H.264 encoder is unavailable")?;
            let mut stream = output.add_stream(codec).map_err(|e| e.to_string())?;
            stream.set_time_base(Rational(1, settings.fps as i32));
            stream.set_avg_frame_rate(Rational(settings.fps as i32, 1));
            let mut context = codec::context::Context::new_with_codec(codec)
                .encoder()
                .video()
                .map_err(|e| e.to_string())?;
            context.set_width(width);
            context.set_height(height);
            context.set_format(if animation {
                Pixel::BGRZ
            } else {
                Pixel::YUV420P
            });
            context.set_time_base(Rational(1, settings.fps as i32));
            context.set_frame_rate(Some(Rational(settings.fps as i32, 1)));
            context.set_flags(ffmpeg::codec::flag::Flags::GLOBAL_HEADER);
            let mut options = Dictionary::new();
            options.set("threads", &threads.max(1).to_string());
            if animation {
                options.set("preset", "ultrafast");
                options.set("qp", "0");
            } else {
                let rate = u64::from(width) * u64::from(height) * u64::from(settings.fps);
                options.set(
                    "preset",
                    if rate > 1920 * 1080 * 30 {
                        "ultrafast"
                    } else {
                        "veryfast"
                    },
                );
                options.set("tune", "zerolatency");
                options.set("crf", "19");
            }
            let video_encoder = context.open_with(options).map_err(|e| e.to_string())?;
            stream.set_parameters(&video_encoder);
            encoder = Some(video_encoder);
        }
        let mut tracks = Vec::new();
        if audio {
            tracks.push(AudioTrack::new(&mut output, mp3)?);
        }
        output
            .write_header()
            .map_err(|e| format!("FFmpeg header: {e}"))?;
        let stream_tb = if video {
            output.stream(0).ok_or("Missing video stream")?.time_base()
        } else {
            Rational(1, settings.fps as i32)
        };
        for track in &mut tracks {
            track.stream_tb = output
                .stream(track.stream_index)
                .ok_or("Missing audio stream")?
                .time_base();
        }
        let encoder_tb = encoder
            .as_ref()
            .map(|e| e.time_base())
            .unwrap_or(Rational(1, settings.fps as i32));
        let scaler = if video {
            Some(
                scaling::Context::get(
                    Pixel::BGRA,
                    rect.w,
                    rect.h,
                    encoder.as_ref().unwrap().format(),
                    width,
                    height,
                    scaling::flag::Flags::FAST_BILINEAR,
                )
                .map_err(|e| format!("FFmpeg scaler: {e}"))?,
            )
        } else {
            None
        };
        Ok(Self {
            output,
            encoder,
            scaler,
            source: frame::Video::new(Pixel::BGRA, rect.w, rect.h),
            converted: frame::Video::empty(),
            audio: tracks,
            next_pts: 0,
            video_packets: 0,
            encoder_tb,
            stream_tb,
            closed: false,
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn push(&mut self, data: &Frame) -> Result<(), String> {
        let pts = self.next_pts;
        self.push_at(data, pts)
    }

    pub(crate) fn push_at(&mut self, data: &Frame, pts: i64) -> Result<(), String> {
        let Some(encoder) = &mut self.encoder else {
            return Ok(());
        };
        copy_bgra(data, &mut self.source)?;
        self.scaler
            .as_mut()
            .unwrap()
            .run(&self.source, &mut self.converted)
            .map_err(|e| e.to_string())?;
        self.converted.set_pts(Some(pts));
        self.next_pts = pts.saturating_add(1);
        encoder
            .send_frame(&self.converted)
            .map_err(|e| e.to_string())?;
        self.drain()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn push_audio(&mut self, samples: &[f32]) -> Result<(), String> {
        let start = self
            .audio
            .first()
            .map(|track| track.pending_end())
            .unwrap_or(0);
        self.push_audio_at(start, samples)
    }

    pub(crate) fn push_audio_at(
        &mut self,
        start_sample: u64,
        samples: &[f32],
    ) -> Result<(), String> {
        for track in &mut self.audio {
            track.push_from(start_sample, samples, &mut self.output)?;
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), String> {
        let mut packet = Packet::empty();
        while self
            .encoder
            .as_mut()
            .unwrap()
            .receive_packet(&mut packet)
            .is_ok()
        {
            packet.set_stream(0);
            packet.rescale_ts(self.encoder_tb, self.stream_tb);
            packet
                .write_interleaved(&mut self.output)
                .map_err(|e| e.to_string())?;
            self.video_packets += 1;
        }
        Ok(())
    }

    fn close(&mut self) -> Result<Written, String> {
        if self.closed {
            return Ok(Written {
                audio_packets: self.audio.iter().map(|track| track.packets).sum(),
                video_packets: self.video_packets,
            });
        }
        self.closed = true;
        let mut error = None;
        if let Some(encoder) = &mut self.encoder {
            if let Err(e) = encoder.send_eof() {
                error = Some(e.to_string());
            }
            if let Err(e) = self.drain() {
                error = Some(e);
            }
        }
        for index in 0..self.audio.len() {
            if let Err(e) = self.audio[index].finish(&mut self.output) {
                error = Some(e);
            }
        }
        if let Err(e) = self.output.write_trailer() {
            error = Some(e.to_string());
        }
        let written = Written {
            audio_packets: self.audio.iter().map(|track| track.packets).sum(),
            video_packets: self.video_packets,
        };
        match error {
            Some(error) => Err(error),
            None => Ok(written),
        }
    }

    pub(crate) fn finish(mut self) -> Result<Written, String> {
        self.close()
    }
}

impl Drop for VideoWriter {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.close();
        }
    }
}

pub use crate::capture::{Finalizer, Session};

pub(crate) fn convert_animation(
    input: &Path,
    output: &Path,
    kind: Format,
    fps: u32,
    height: u32,
) -> Result<(), String> {
    let spec = if kind == Format::Gif {
        crate::geom::gif_single_graph(fps, height)
    } else {
        crate::geom::webp_graph(fps, height)
    };
    let two_pass = kind == Format::Gif && gif_is_large(input, fps)?;
    if two_pass {
        let palette = collect_palette(input, &crate::geom::gif_palette_graph(fps, height))?;
        encode_palette(
            input,
            output,
            fps,
            &palette,
            &crate::geom::gif_use_graph(fps, height),
        )?;
    } else {
        encode_chain(input, output, kind, fps, &spec)?;
    }
    if input != output {
        let _ = std::fs::remove_file(input);
    }
    Ok(())
}

fn gif_is_large(input: &Path, fps: u32) -> Result<bool, String> {
    let source = format::input(input).map_err(|error| error.to_string())?;
    let stream = source
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or("Missing animation source")?;
    let decoder = codec::context::Context::from_parameters(stream.parameters())
        .map_err(|error| error.to_string())?
        .decoder()
        .video()
        .map_err(|error| error.to_string())?;
    let duration = source.duration().max(0) as u64;
    Ok(duration == 0
        || crate::geom::gif_needs_two_pass(decoder.width(), decoder.height(), duration, fps))
}

struct Opened {
    input: format::context::Input,
    decoder: codec::decoder::Video,
    index: usize,
    time_base: Rational,
}

fn open_source(path: &Path) -> Result<Opened, String> {
    let input = format::input(path).map_err(|error| error.to_string())?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or("Missing animation source")?;
    let index = stream.index();
    let time_base = stream.time_base();
    let decoder = codec::context::Context::from_parameters(stream.parameters())
        .map_err(|error| error.to_string())?
        .decoder()
        .video()
        .map_err(|error| error.to_string())?;
    Ok(Opened {
        input,
        decoder,
        index,
        time_base,
    })
}

fn collect_palette(path: &Path, spec: &str) -> Result<frame::Video, String> {
    let mut opened = open_source(path)?;
    let mut graph = graph_for(
        &opened,
        &[(
            "in",
            opened.decoder.width(),
            opened.decoder.height(),
            opened.decoder.format(),
        )],
    )?;
    graph
        .output("in", 0)
        .map_err(|error| error.to_string())?
        .input("out", 0)
        .map_err(|error| error.to_string())?
        .parse(spec)
        .map_err(|error| error.to_string())?;
    graph.validate().map_err(|error| error.to_string())?;
    feed(&mut opened, &mut graph, "in")?;
    source_eof(&mut graph, "in")?;
    let mut palette = frame::Video::empty();
    graph
        .get("out")
        .ok_or("Missing filter output")?
        .sink()
        .frame(&mut palette)
        .map_err(|error| error.to_string())?;
    Ok(palette)
}

fn encode_palette(
    path: &Path,
    output: &Path,
    fps: u32,
    palette: &frame::Video,
    spec: &str,
) -> Result<(), String> {
    let mut opened = open_source(path)?;
    let mut graph = graph_for(
        &opened,
        &[
            (
                "in",
                opened.decoder.width(),
                opened.decoder.height(),
                opened.decoder.format(),
            ),
            ("pal", palette.width(), palette.height(), palette.format()),
        ],
    )?;
    graph
        .output("in", 0)
        .map_err(|error| error.to_string())?
        .output("pal", 0)
        .map_err(|error| error.to_string())?
        .input("out", 0)
        .map_err(|error| error.to_string())?
        .parse(spec)
        .map_err(|error| error.to_string())?;
    graph.validate().map_err(|error| error.to_string())?;
    {
        let mut input = graph.get("pal").ok_or("Missing palette input")?;
        let mut source = input.source();
        source.add(palette).map_err(|error| error.to_string())?;
        source.flush().map_err(|error| error.to_string())?;
    }
    let mut encoder = None;
    feed(&mut opened, &mut graph, "in")?;
    source_eof(&mut graph, "in")?;
    pull(&mut graph, output, Format::Gif, fps, &mut encoder)?;
    finish_encoder(encoder)
}

fn encode_chain(
    path: &Path,
    output: &Path,
    kind: Format,
    fps: u32,
    spec: &str,
) -> Result<(), String> {
    let mut opened = open_source(path)?;
    let mut graph = graph_for(
        &opened,
        &[(
            "in",
            opened.decoder.width(),
            opened.decoder.height(),
            opened.decoder.format(),
        )],
    )?;
    graph
        .output("in", 0)
        .map_err(|error| error.to_string())?
        .input("out", 0)
        .map_err(|error| error.to_string())?
        .parse(spec)
        .map_err(|error| error.to_string())?;
    graph.validate().map_err(|error| error.to_string())?;
    let mut encoder = None;
    let mut packet = Packet::empty();
    loop {
        let read = unsafe {
            // SAFETY: Both pointers belong to the open ffmpeg input and its packet. A negative code ends the read.
            ffmpeg::ffi::av_read_frame(opened.input.as_mut_ptr(), packet.as_mut_ptr())
        };
        if read < 0 {
            break;
        }
        if packet.stream() == opened.index {
            opened
                .decoder
                .send_packet(&packet)
                .map_err(|error| error.to_string())?;
            let mut decoded = frame::Video::empty();
            while opened.decoder.receive_frame(&mut decoded).is_ok() {
                graph
                    .get("in")
                    .ok_or("Missing filter input")?
                    .source()
                    .add(&decoded)
                    .map_err(|error| error.to_string())?;
                pull(&mut graph, output, kind, fps, &mut encoder)?;
            }
        }
    }
    opened
        .decoder
        .send_eof()
        .map_err(|error| error.to_string())?;
    let mut decoded = frame::Video::empty();
    while opened.decoder.receive_frame(&mut decoded).is_ok() {
        graph
            .get("in")
            .ok_or("Missing filter input")?
            .source()
            .add(&decoded)
            .map_err(|error| error.to_string())?;
        pull(&mut graph, output, kind, fps, &mut encoder)?;
    }
    source_eof(&mut graph, "in")?;
    pull(&mut graph, output, kind, fps, &mut encoder)?;
    finish_encoder(encoder)
}

fn graph_for(
    opened: &Opened,
    inputs: &[(&str, u32, u32, Pixel)],
) -> Result<ffmpeg::filter::Graph, String> {
    let mut graph = ffmpeg::filter::Graph::new();
    let buffer = ffmpeg::filter::find("buffer").ok_or("FFmpeg buffer filter is unavailable")?;
    let sink =
        ffmpeg::filter::find("buffersink").ok_or("FFmpeg buffersink filter is unavailable")?;
    let base = if opened.time_base.denominator() == 0 {
        Rational(1, 30)
    } else {
        opened.time_base
    };
    for (name, width, height, pixel) in inputs {
        let args = format!(
            "video_size={width}x{height}:pix_fmt={}:time_base={}/{}:pixel_aspect=1/1",
            pixel_name(*pixel),
            base.numerator(),
            base.denominator()
        );
        graph
            .add(&buffer, name, &args)
            .map_err(|error| error.to_string())?;
    }
    graph
        .add(&sink, "out", "")
        .map_err(|error| error.to_string())?;
    Ok(graph)
}

fn feed(
    opened: &mut Opened,
    graph: &mut ffmpeg::filter::Graph,
    input_name: &str,
) -> Result<(), String> {
    let mut packet = Packet::empty();
    loop {
        let read = unsafe {
            // SAFETY: Both pointers belong to the open ffmpeg input and its packet. A negative code ends the read.
            ffmpeg::ffi::av_read_frame(opened.input.as_mut_ptr(), packet.as_mut_ptr())
        };
        if read < 0 {
            break;
        }
        if packet.stream() == opened.index {
            opened
                .decoder
                .send_packet(&packet)
                .map_err(|error| error.to_string())?;
            let mut decoded = frame::Video::empty();
            while opened.decoder.receive_frame(&mut decoded).is_ok() {
                graph
                    .get(input_name)
                    .ok_or("Missing filter input")?
                    .source()
                    .add(&decoded)
                    .map_err(|error| error.to_string())?;
            }
        }
    }
    opened
        .decoder
        .send_eof()
        .map_err(|error| error.to_string())?;
    let mut decoded = frame::Video::empty();
    while opened.decoder.receive_frame(&mut decoded).is_ok() {
        graph
            .get(input_name)
            .ok_or("Missing filter input")?
            .source()
            .add(&decoded)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn source_eof(graph: &mut ffmpeg::filter::Graph, name: &str) -> Result<(), String> {
    graph
        .get(name)
        .ok_or("Missing filter input")?
        .source()
        .flush()
        .map_err(|error| error.to_string())
}

struct AnimationEncoder {
    output: format::context::Output,
    encoder: encoder::video::Encoder,
    time_base: Rational,
    output_base: Rational,
    next_pts: i64,
    step: i64,
}

fn pull(
    graph: &mut ffmpeg::filter::Graph,
    output: &Path,
    kind: Format,
    fps: u32,
    encoder: &mut Option<AnimationEncoder>,
) -> Result<(), String> {
    loop {
        let mut frame = frame::Video::empty();
        let ready = graph
            .get("out")
            .ok_or("Missing filter output")?
            .sink()
            .frame(&mut frame)
            .is_ok();
        if !ready {
            break;
        }
        if encoder.is_none() {
            *encoder = Some(AnimationEncoder::open(output, kind, &frame, fps)?);
        }
        encoder.as_mut().unwrap().push(&mut frame)?;
    }
    Ok(())
}

fn finish_encoder(encoder: Option<AnimationEncoder>) -> Result<(), String> {
    match encoder {
        Some(encoder) => encoder.finish(),
        None => Err("Animation filter produced no frames".into()),
    }
}

impl AnimationEncoder {
    fn open(path: &Path, kind: Format, frame: &frame::Video, fps: u32) -> Result<Self, String> {
        let codec = if kind == Format::Gif {
            encoder::find(codec::Id::GIF)
        } else {
            encoder::find_by_name("libwebp_anim")
        }
        .ok_or("Animation encoder is unavailable")?;
        let mut output = format::output(path).map_err(|error| error.to_string())?;
        let time_base = if kind == Format::Gif {
            Rational(1, 100)
        } else {
            Rational(1, 1000)
        };
        let mut stream = output
            .add_stream(codec)
            .map_err(|error| error.to_string())?;
        stream.set_time_base(time_base);
        stream.set_avg_frame_rate(Rational(fps as i32, 1));
        let mut context = codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()
            .map_err(|error| error.to_string())?;
        context.set_width(frame.width() & !1);
        context.set_height(frame.height() & !1);
        context.set_format(frame.format());
        context.set_time_base(time_base);
        context.set_frame_rate(Some(Rational(fps as i32, 1)));
        let mut options = Dictionary::new();
        if kind == Format::Webp {
            options.set("lossless", "0");
            options.set("quality", "85");
            options.set("compression_level", "4");
        }
        let encoder = context
            .open_with(options)
            .map_err(|error| error.to_string())?;
        stream.set_parameters(&encoder);
        let mut mux = Dictionary::new();
        mux.set("loop", "0");
        output
            .write_header_with(mux)
            .map_err(|error| error.to_string())?;
        let output_base = output
            .stream(0)
            .ok_or("Missing animation stream")?
            .time_base();
        let step = (time_base.denominator() as f64 / fps.max(1) as f64)
            .round()
            .max(1.0) as i64;
        Ok(Self {
            output,
            encoder,
            time_base,
            output_base,
            next_pts: 0,
            step,
        })
    }

    fn push(&mut self, frame: &mut frame::Video) -> Result<(), String> {
        frame.set_pts(Some(self.next_pts));
        self.next_pts += self.step;
        self.encoder
            .send_frame(frame)
            .map_err(|error| error.to_string())?;
        let mut packet = Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(0);
            packet.rescale_ts(self.time_base, self.output_base);
            packet
                .write_interleaved(&mut self.output)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn finish(mut self) -> Result<(), String> {
        self.encoder.send_eof().map_err(|error| error.to_string())?;
        let mut packet = Packet::empty();
        while self.encoder.receive_packet(&mut packet).is_ok() {
            packet.set_stream(0);
            packet.rescale_ts(self.time_base, self.output_base);
            packet
                .write_interleaved(&mut self.output)
                .map_err(|error| error.to_string())?;
        }
        self.output
            .write_trailer()
            .map_err(|error| error.to_string())
    }
}

fn pixel_name(pixel: Pixel) -> String {
    pixel
        .descriptor()
        .map(|item| item.name().to_string())
        .unwrap_or_else(|| "bgra".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("ezcap-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn test_frame() -> Frame {
        let mut data = Vec::new();
        for y in 0..64 {
            for x in 0..64 {
                data.extend_from_slice(&[(x * 3) as u8, (y * 3) as u8, 80, 255]);
            }
        }
        Frame {
            width: 64,
            height: 64,
            data,
        }
    }

    fn write_audio(writer: &mut VideoWriter, seconds: usize) {
        let samples = vec![0.1f32; 48000 * 2 * seconds];
        writer.push_audio(&samples).unwrap();
    }

    fn decoded_audio_samples(path: &Path) -> usize {
        let mut input = format::input(path).unwrap();
        let stream = input.streams().best(ffmpeg::media::Type::Audio).unwrap();
        let index = stream.index();
        let mut decoder = codec::context::Context::from_parameters(stream.parameters())
            .unwrap()
            .decoder()
            .audio()
            .unwrap();
        let mut decoded = frame::Audio::empty();
        let mut samples = 0usize;
        for (source, packet) in input.packets() {
            if source.index() != index {
                continue;
            }
            decoder.send_packet(&packet).unwrap();
            while decoder.receive_frame(&mut decoded).is_ok() {
                samples += decoded.samples();
            }
        }
        decoder.send_eof().unwrap();
        while decoder.receive_frame(&mut decoded).is_ok() {
            samples += decoded.samples();
        }
        samples
    }

    #[test]
    fn writes_real_mkv_with_audio_in_one_pass() {
        init();
        let dir = directory("mkv");
        let path = dir.join("video.mkv");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 64,
            h: 64,
        };
        let mut writer = VideoWriter::new(
            &path,
            rect,
            VideoSettings {
                height: 64,
                fps: 30,
            },
            Format::Mkv,
            1,
            true,
            true,
        )
        .unwrap();
        for _ in 0..5 {
            writer.push(&test_frame()).unwrap();
        }
        write_audio(&mut writer, 1);
        writer.finish().unwrap();
        let input = format::input(&path).unwrap();
        let stream = input.streams().best(ffmpeg::media::Type::Video).unwrap();
        let decoder = codec::context::Context::from_parameters(stream.parameters())
            .unwrap()
            .decoder()
            .video()
            .unwrap();
        assert_eq!((decoder.width(), decoder.height()), (64, 64));
        assert!(input.streams().best(ffmpeg::media::Type::Audio).is_some());
        drop(input);
        assert!(
            decoded_audio_samples(&path) >= 40000,
            "mkv audio is not decodable"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_real_animations() {
        init();
        let dir = directory("animations");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 64,
            h: 64,
        };
        for kind in [Format::Gif, Format::Webp] {
            let scratch = dir.join(format!("{}.mkv", kind.extension()));
            let output = dir.join(format!("result.{}", kind.extension()));
            let mut writer = VideoWriter::new(
                &scratch,
                rect,
                VideoSettings {
                    height: 64,
                    fps: 30,
                },
                kind,
                1,
                true,
                false,
            )
            .unwrap();
            for _ in 0..5 {
                writer.push(&test_frame()).unwrap();
            }
            writer.finish().unwrap();
            convert_animation(&scratch, &output, kind, 30, 64).unwrap();
            assert!(!scratch.exists());
            let input = format::input(&output).unwrap();
            assert!(input.streams().best(ffmpeg::media::Type::Video).is_some());
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_real_aac() {
        init();
        let dir = directory("aac");
        let output = dir.join("audio.mkv");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 64,
            h: 64,
        };
        let mut writer = VideoWriter::new(
            &output,
            rect,
            VideoSettings {
                height: 64,
                fps: 30,
            },
            Format::Mkv,
            1,
            false,
            true,
        )
        .unwrap();
        write_audio(&mut writer, 1);
        writer.finish().unwrap();
        assert!(
            decoded_audio_samples(&output) >= 40000,
            "aac audio is not decodable"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_real_mp3() {
        init();
        let dir = directory("mp3");
        let output = dir.join("audio.mp3");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 64,
            h: 64,
        };
        let mut writer = VideoWriter::new(
            &output,
            rect,
            VideoSettings {
                height: 64,
                fps: 30,
            },
            Format::Mkv,
            1,
            false,
            true,
        )
        .unwrap();
        write_audio(&mut writer, 1);
        writer.finish().unwrap();
        assert!(
            decoded_audio_samples(&output) >= 40000,
            "mp3 audio is not decodable"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn uneven_mp3_is_still_decodable_when_the_writer_is_dropped() {
        init();
        let dir = directory("mp3-chunks");
        let output = dir.join("audio.mp3");
        {
            let rect = Rect {
                x: 0,
                y: 0,
                w: 16,
                h: 16,
            };
            let mut writer = VideoWriter::new(
                &output,
                rect,
                VideoSettings {
                    height: 16,
                    fps: 30,
                },
                Format::Mkv,
                1,
                false,
                true,
            )
            .unwrap();
            for size in [100, 2000, 50, 8000, 333] {
                writer.push_audio(&vec![0.2f32; size * 2]).unwrap();
            }
        }
        assert!(
            decoded_audio_samples(&output) > 4_000,
            "an unfinished mp3 must still be a valid file"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_audio_file_contains_no_packets() {
        init();
        let dir = directory("mp3-empty");
        let output = dir.join("audio.mp3");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 16,
            h: 16,
        };
        let writer = VideoWriter::new(
            &output,
            rect,
            VideoSettings {
                height: 16,
                fps: 30,
            },
            Format::Mkv,
            1,
            false,
            true,
        )
        .unwrap();
        let written = writer.finish().unwrap();
        assert_eq!(
            written.audio_packets, 0,
            "a take with no samples must not pretend it has audio"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn audio_gap_is_kept_in_the_timeline() {
        init();
        let dir = directory("mp3-gap");
        let output = dir.join("audio.mp3");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 16,
            h: 16,
        };
        let mut writer = VideoWriter::new(
            &output,
            rect,
            VideoSettings {
                height: 16,
                fps: 30,
            },
            Format::Mkv,
            1,
            false,
            true,
        )
        .unwrap();
        writer.push_audio_at(0, &vec![0.2f32; 12_000 * 2]).unwrap();
        writer
            .push_audio_at(48_000, &vec![0.2f32; 12_000 * 2])
            .unwrap();
        writer.finish().unwrap();
        assert!(
            decoded_audio_samples(&output) >= 48_000,
            "a later microphone chunk must not be glued to the front"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn video_frames_keep_the_slots_they_were_given() {
        init();
        let dir = directory("mkv-slots");
        let path = dir.join("video.mkv");
        let rect = Rect {
            x: 0,
            y: 0,
            w: 64,
            h: 64,
        };
        let mut writer = VideoWriter::new(
            &path,
            rect,
            VideoSettings {
                height: 64,
                fps: 60,
            },
            Format::Mkv,
            1,
            true,
            false,
        )
        .unwrap();
        for slot in [0, 10, 20, 30] {
            writer.push_at(&test_frame(), slot).unwrap();
        }
        let written = writer.finish().unwrap();
        assert!(written.video_packets > 0);
        let mut input = format::input(&path).unwrap();
        let stream = input.streams().best(ffmpeg::media::Type::Video).unwrap();
        let index = stream.index();
        let mut decoder = codec::context::Context::from_parameters(stream.parameters())
            .unwrap()
            .decoder()
            .video()
            .unwrap();
        let mut decoded = frame::Video::empty();
        let mut stamps = Vec::new();
        for (source, packet) in input.packets() {
            if source.index() != index {
                continue;
            }
            decoder.send_packet(&packet).unwrap();
            while decoder.receive_frame(&mut decoded).is_ok() {
                stamps.push(decoded.pts());
            }
        }
        decoder.send_eof().unwrap();
        while decoder.receive_frame(&mut decoded).is_ok() {
            stamps.push(decoded.pts());
        }
        let pts: Vec<i64> = stamps
            .into_iter()
            .map(|stamp| stamp.expect("frame timestamp"))
            .collect();
        assert_eq!(pts.len(), 4);
        assert_eq!(pts[0], 0);
        let step = pts[1];
        assert!(step > 0, "slotted frames collapsed onto the same timestamp");
        assert!((pts[2] - pts[1] - step).abs() <= 1);
        assert!((pts[3] - pts[2] - step).abs() <= 1);
        drop(decoded);
        drop(decoder);
        drop(input);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn output_names_have_the_expected_prefixes() {
        for (kind, extension, prefix) in [
            ("Video", "mkv", "Video_"),
            ("Animation", "gif", "Animation_"),
            ("Animation", "webp", "Animation_"),
            ("Image", "png", "Image_"),
            ("Audio", "mp3", "Audio_"),
        ] {
            let path = output_path(kind, extension).unwrap();
            let name = path.file_name().unwrap().to_string_lossy();
            assert!(name.starts_with(prefix), "{name}");
            assert!(name.ends_with(&format!(".{extension}")), "{name}");
        }
    }
}
