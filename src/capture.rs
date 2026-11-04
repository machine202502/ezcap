//! Screen and audio capture.
//!
//! Windows Graphics Capture and WASAPI streams exist only while a recording
//! or a still grab is in progress. Dropping the session stops both, so an
//! idle tray process keeps no capture device, no audio client and no extra frame.

use crate::geom::{self, AudioQueue};
use crate::media::{self, Format, Frame, RecordOptions, Rect};
use crate::wide;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, HDC, HMONITOR, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, GetWindowPlacement, IsIconic,
    IsWindow, IsWindowVisible, TrackPopupMenu, MF_SEPARATOR, MF_STRING, SW_MINIMIZE,
    SW_SHOWMINIMIZED, SW_SHOWMINNOACTIVE, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    WINDOWPLACEMENT,
};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};
use windows_capture::window::Window;

struct Slot {
    frame: Mutex<Option<Frame>>,
    arrived: Condvar,
}

struct Handler {
    slot: Arc<Slot>,
    crop: Option<(u32, u32, u32, u32)>,
    scratch: Vec<u8>,
}

impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Arc<Slot>, Option<(u32, u32, u32, u32)>);
    type Error = String;

    fn new(ctx: Context<Self::Flags>) -> Result<Self, Self::Error> {
        let (slot, crop) = ctx.flags;
        Ok(Self {
            slot,
            crop,
            scratch: Vec::new(),
        })
    }

    fn on_frame_arrived(
        &mut self,
        frame: &mut WgcFrame,
        _control: InternalCaptureControl,
    ) -> Result<(), Self::Error> {
        let buffer = match self.crop {
            Some((x, y, w, h)) => {
                let max_w = frame.width();
                let max_h = frame.height();
                let x = x.min(max_w.saturating_sub(2));
                let y = y.min(max_h.saturating_sub(2));
                let w = (w.min(max_w - x)) & !1;
                let h = (h.min(max_h - y)) & !1;
                if w < 2 || h < 2 {
                    frame.buffer().map_err(|error| error.to_string())?
                } else {
                    frame
                        .buffer_crop(x, y, x + w, y + h)
                        .map_err(|error| error.to_string())?
                }
            }
            None => frame.buffer().map_err(|error| error.to_string())?,
        };
        let width = buffer.width();
        let height = buffer.height();
        let mut data = buffer.as_nopadding_buffer(&mut self.scratch).to_vec();
        for pixel in data.as_chunks_mut::<4>().0 {
            pixel[3] = 255;
        }
        let mut guard = self
            .slot
            .frame
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *guard = Some(Frame {
            width,
            height,
            data,
        });
        self.slot.arrived.notify_all();
        Ok(())
    }
}

struct VideoCapture {
    control: Option<CaptureControl<Handler, String>>,
    slot: Arc<Slot>,
    window: Option<Window>,
}

impl VideoCapture {
    fn start(
        monitor: Monitor,
        crop: Option<(u32, u32, u32, u32)>,
        fps: u32,
    ) -> Result<(Self, Frame), String> {
        let slot = Arc::new(Slot {
            frame: Mutex::new(None),
            arrived: Condvar::new(),
        });
        let interval = Duration::from_secs_f64(1.0 / fps.max(1) as f64);
        let settings = |border| {
            Settings::new(
                monitor,
                CursorCaptureSettings::WithCursor,
                border,
                SecondaryWindowSettings::Exclude,
                MinimumUpdateIntervalSettings::Custom(interval),
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                (slot.clone(), crop),
            )
        };
        let control = Handler::start_free_threaded(settings(DrawBorderSettings::WithoutBorder))
            .or_else(|_| Handler::start_free_threaded(settings(DrawBorderSettings::Default)))
            .map_err(|error| format!("Windows Graphics Capture: {error}"))?;
        let capture = Self {
            control: Some(control),
            slot: slot.clone(),
            window: None,
        };
        let frame = wait_frame(&slot, Duration::from_secs(3))?;
        Ok((capture, frame))
    }

    fn start_window(window: Window, fps: u32) -> Result<(Self, Frame), String> {
        let slot = Arc::new(Slot {
            frame: Mutex::new(None),
            arrived: Condvar::new(),
        });
        let interval = Duration::from_secs_f64(1.0 / fps.max(1) as f64);
        let settings = |border| {
            Settings::new(
                window,
                CursorCaptureSettings::WithCursor,
                border,
                SecondaryWindowSettings::Exclude,
                MinimumUpdateIntervalSettings::Custom(interval),
                DirtyRegionSettings::Default,
                ColorFormat::Bgra8,
                (slot.clone(), None),
            )
        };
        let control = Handler::start_free_threaded(settings(DrawBorderSettings::WithoutBorder))
            .or_else(|_| Handler::start_free_threaded(settings(DrawBorderSettings::Default)))
            .map_err(|error| format!("Windows Graphics Capture: {error}"))?;
        let capture = Self {
            control: Some(control),
            slot: slot.clone(),
            window: Some(window),
        };
        let frame = wait_frame(&slot, Duration::from_secs(3))?;
        Ok((capture, frame))
    }

    fn capture_alive(&self) -> bool {
        self.control
            .as_ref()
            .is_some_and(|control| !control.is_finished())
    }

    fn available(&self) -> bool {
        self.capture_alive() && self.window.is_none_or(window_usable)
    }

    fn latest(&self) -> Option<Frame> {
        self.slot
            .frame
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    fn stop(&mut self) {
        if let Some(control) = self.control.take() {
            let _ = control.stop();
        }
    }
}

impl Drop for VideoCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn wait_frame(slot: &Slot, timeout: Duration) -> Result<Frame, String> {
    let mut guard = slot.frame.lock().unwrap_or_else(|error| error.into_inner());
    let start = Instant::now();
    while guard.is_none() {
        let remaining = timeout.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            return Err("Windows Graphics Capture did not deliver a frame".into());
        }
        let (next, status) = slot
            .arrived
            .wait_timeout(guard, remaining)
            .unwrap_or_else(|error| error.into_inner());
        guard = next;
        if status.timed_out() && guard.is_none() {
            return Err("Windows Graphics Capture did not deliver a frame".into());
        }
    }
    guard
        .as_ref()
        .cloned()
        .ok_or_else(|| "Windows Graphics Capture did not deliver a frame".into())
}

fn monitor_of(rect: Rect) -> (Monitor, i32, i32, u32, u32) {
    // SAFETY: any screen point is a valid query. `info.cbSize` is set before the call, and the returned handle is only used as an HMONITOR value.
    unsafe {
        let point = POINT {
            x: rect.x + rect.w as i32 / 2,
            y: rect.y + rect.h as i32 / 2,
        };
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(monitor, &mut info).ok();
        let origin_x = info.rcMonitor.left;
        let origin_y = info.rcMonitor.top;
        let width = (info.rcMonitor.right - info.rcMonitor.left).max(0) as u32;
        let height = (info.rcMonitor.bottom - info.rcMonitor.top).max(0) as u32;
        (
            Monitor::from_raw_hmonitor(monitor.0),
            origin_x,
            origin_y,
            width,
            height,
        )
    }
}

fn crop_for(
    rect: Rect,
    origin_x: i32,
    origin_y: i32,
    monitor_w: u32,
    monitor_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let x = (rect.x - origin_x).max(0) as u32;
    let y = (rect.y - origin_y).max(0) as u32;
    if x == 0 && y == 0 && rect.w >= monitor_w && rect.h >= monitor_h {
        None
    } else {
        Some((x, y, rect.w, rect.h))
    }
}

fn fit(frame: Frame, width: u32, height: u32) -> Frame {
    let width = width & !1;
    let height = height & !1;
    if frame.width == width && frame.height == height {
        return frame;
    }
    let mut data = vec![0u8; width as usize * height as usize * 4];
    let copy_w = (frame.width.min(width)) as usize;
    let copy_h = (frame.height.min(height)) as usize;
    for y in 0..copy_h {
        let source = y * frame.width as usize * 4;
        let target = y * width as usize * 4;
        data[target..target + copy_w * 4].copy_from_slice(&frame.data[source..source + copy_w * 4]);
    }
    Frame {
        width,
        height,
        data,
    }
}

/// A copy of the screen that does not open a second capture session.
pub fn grab_display(rect: Rect) -> Result<Frame, String> {
    let mut rect = rect;
    rect.w &= !1;
    rect.h &= !1;
    if rect.w == 0 || rect.h == 0 {
        return Err("The capture area is empty".into());
    }
    media::grab(rect)
}

pub fn grab_still(rect: Rect) -> Result<Frame, String> {
    let mut rect = rect;
    rect.w &= !1;
    rect.h &= !1;
    let (monitor, origin_x, origin_y, monitor_w, monitor_h) = monitor_of(rect);
    match VideoCapture::start(
        monitor,
        crop_for(rect, origin_x, origin_y, monitor_w, monitor_h),
        30,
    ) {
        Ok((mut capture, frame)) => {
            capture.stop();
            Ok(fit(frame, rect.w, rect.h))
        }
        Err(_) => media::grab(rect),
    }
}

struct AudioIn {
    queue: Arc<Mutex<AudioQueue>>,
    _stream: cpal::Stream,
}

fn open_stream(system: bool, device_name: Option<&str>, clock: Instant) -> Result<AudioIn, String> {
    let host = cpal::default_host();
    let (device, supported) = if system {
        let device = host
            .default_output_device()
            .ok_or("No playback device for system audio")?;
        let supported = device
            .default_output_config()
            .map_err(|error| format!("System audio: {error}"))?;
        (device, supported)
    } else if let Some(name) = device_name {
        let device = host
            .input_devices()
            .map_err(|error| error.to_string())?
            .find(|device| device.to_string() == name)
            .ok_or_else(|| format!("Microphone \"{name}\" was not found"))?;
        let supported = device
            .default_input_config()
            .map_err(|error| format!("Microphone: {error}"))?;
        (device, supported)
    } else {
        let device = host.default_input_device().ok_or("No microphone")?;
        let supported = device
            .default_input_config()
            .map_err(|error| format!("Microphone: {error}"))?;
        (device, supported)
    };
    let channels = supported.channels();
    let rate = supported.sample_rate();
    let format = supported.sample_format();
    let config = supported.config();
    let queue = Arc::new(Mutex::new(AudioQueue::default()));
    let target = queue.clone();
    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, channels, rate, target, clock),
        SampleFormat::I16 => build::<i16>(&device, config, channels, rate, target, clock),
        SampleFormat::I32 => build::<i32>(&device, config, channels, rate, target, clock),
        SampleFormat::U16 => build::<u16>(&device, config, channels, rate, target, clock),
        other => Err(format!("Unsupported audio format {other}")),
    }?;
    stream
        .play()
        .map_err(|error| format!("Audio stream: {error}"))?;
    Ok(AudioIn {
        queue,
        _stream: stream,
    })
}

fn build<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: u16,
    rate: u32,
    target: Arc<Mutex<AudioQueue>>,
    clock: Instant,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            config,
            move |data, _| {
                let converted: Vec<f32> = data
                    .iter()
                    .map(|sample| sample.to_sample::<f32>())
                    .collect();
                let ready = geom::to_stereo_48k(&converted, channels, rate);
                let frames = ready.len() / 2;
                if frames == 0 {
                    return;
                }
                let end = geom::sample_index(clock.elapsed().as_nanos(), 48_000);
                let start = end.saturating_sub(frames as u64);
                let mut guard = target.lock().unwrap_or_else(|error| error.into_inner());
                guard.append(start, &ready);
                let end = guard.end();
                if end.saturating_sub(guard.origin) > 48_000 * 10 {
                    guard.drop_before(end - 48_000 * 10);
                }
            },
            |_| {},
            None,
        )
        .map_err(|error| format!("Audio stream: {error}"))
}

fn microphone_name(options: &RecordOptions) -> Option<String> {
    let id = options.profile.audio.device.as_str();
    if id == "Auto" {
        None
    } else {
        media::device_name(id)
    }
}

struct Prepared {
    output: PathBuf,
    scratch: PathBuf,
    format: Format,
    fps: u32,
    height: u32,
    animate: bool,
}

fn working_path(output: &Path, animate: bool) -> PathBuf {
    if animate {
        return output.with_extension("working.mkv");
    }
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("ezcap.bin");
    std::env::temp_dir().join(format!("ezcap-{name}"))
}

fn publish_file(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SourceFate {
    Live,
    Card,
    Hold,
    Stop,
}

fn source_fate(tracked: bool, signal: bool, keep: bool, show_card: bool) -> SourceFate {
    if !tracked || signal {
        SourceFate::Live
    } else if !keep {
        SourceFate::Stop
    } else if show_card {
        SourceFate::Card
    } else {
        SourceFate::Hold
    }
}

/// A tracked source still has a picture when a frame just arrived, or when the
/// window is on screen and the capture session is running. A minimized, hidden
/// or closed window with nothing new arriving has no signal.
fn signal_present(tracked: bool, usable: bool, capture_alive: bool, frame_arrived: bool) -> bool {
    if !tracked {
        return true;
    }
    frame_arrived || (usable && capture_alive)
}

fn window_state_usable(
    exists: bool,
    visible: bool,
    iconic: bool,
    cloaked: bool,
    minimized: bool,
    width: i32,
    height: i32,
) -> bool {
    exists && visible && !iconic && !cloaked && !minimized && width >= 16 && height >= 16
}

fn record(
    options: RecordOptions,
    stop: Arc<AtomicBool>,
    ready: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<Prepared, String> {
    match prepare(&options, &stop) {
        Ok(session) => {
            let partial = session.scratch.clone();
            let animate = session.animate;
            let _ = ready.send(Ok(()));
            match session.run(options, stop) {
                Ok(prepared) => Ok(prepared),
                Err(error) => {
                    if !animate {
                        let _ = std::fs::remove_file(&partial);
                    }
                    Err(error)
                }
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error.clone()));
            Err(error)
        }
    }
}

struct Live {
    rect: Rect,
    output: PathBuf,
    scratch: PathBuf,
    animate: bool,
    clock: Instant,
    inputs: Vec<AudioIn>,
    video: Option<VideoCapture>,
    writer: media::VideoWriter,
    last: Option<Frame>,
    recover: Option<Window>,
    resilient: bool,
    show_no_signal: bool,
    show_card: bool,
    placeholder: Option<Frame>,
    next_retry: Instant,
    was_usable: bool,
}

impl Live {
    fn try_recover(&mut self, fps: u32, force: bool) {
        let Some(window) = self.recover else {
            if !force
                && self
                    .video
                    .as_ref()
                    .is_some_and(|source| source.capture_alive())
            {
                return;
            }
            if let Some(source) = &mut self.video {
                source.stop();
            }
            if let Ok((capture, _)) = VideoCapture::start(monitor_of(self.rect).0, None, fps) {
                self.video = Some(capture);
            }
            return;
        };
        if !window_usable(window) {
            return;
        }
        if !force && self.video.as_ref().is_some_and(|source| source.available()) {
            return;
        }
        if let Some(source) = &mut self.video {
            source.stop();
        }
        if let Ok((capture, _)) = VideoCapture::start_window(window, fps) {
            self.video = Some(capture);
        } else {
            self.video = None;
        }
    }

    fn pull_audio(&mut self, cursor: &mut u64, record_from: u64, until: u64) -> Result<(), String> {
        loop {
            if *cursor >= until {
                return Ok(());
            }
            let mut queues = Vec::with_capacity(self.inputs.len());
            for input in &self.inputs {
                queues.push(
                    input
                        .queue
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .clone(),
                );
            }
            let mixed = geom::mix_from(&mut queues, *cursor, until, 48_000);
            if mixed.is_empty() {
                return Ok(());
            }
            let frames = (mixed.len() / 2) as u64;
            self.writer
                .push_audio_at(cursor.saturating_sub(record_from), &mixed)?;
            *cursor += frames;
            for input in &self.inputs {
                input
                    .queue
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .drop_before(*cursor);
            }
            if frames < 48_000 {
                return Ok(());
            }
        }
    }

    fn run(mut self, options: RecordOptions, stop: Arc<AtomicBool>) -> Result<Prepared, String> {
        let origin_ns = self.clock.elapsed().as_nanos();
        let record_from = geom::sample_index(origin_ns, 48_000);
        for input in &self.inputs {
            input
                .queue
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .drop_before(record_from);
        }
        let mut cursor = record_from;
        let mut last_slot = -1i64;
        let fps = options.profile.video.fps.max(1) as u64;
        let frame_time = Duration::from_nanos(1_000_000_000 / fps);
        let mut next_deadline = Instant::now();
        let loop_result = loop {
            let horizon = geom::sample_index(self.clock.elapsed().as_nanos(), 48_000)
                .saturating_sub(48_000 / 5);
            if let Err(error) = self.pull_audio(&mut cursor, record_from, horizon) {
                break Err(error);
            }
            if !options.audio_only {
                let usable = self.recover.map(window_usable).unwrap_or(true);
                let resumed = self.resilient && usable && !self.was_usable;
                self.was_usable = usable;
                let alive = self
                    .video
                    .as_ref()
                    .is_some_and(|source| source.capture_alive());
                if self.resilient
                    && usable
                    && (resumed || !alive)
                    && Instant::now() >= self.next_retry
                {
                    self.try_recover(options.profile.video.fps, resumed);
                    self.next_retry = Instant::now() + Duration::from_millis(250);
                }
                let arrived = self.video.as_mut().and_then(|source| source.latest());
                let alive = self
                    .video
                    .as_ref()
                    .is_some_and(|source| source.capture_alive());
                let signal = signal_present(self.resilient, usable, alive, arrived.is_some());
                let fate = source_fate(self.resilient, signal, self.show_no_signal, self.show_card);
                if let Some(frame) = arrived {
                    if fate == SourceFate::Live {
                        self.last = Some(if self.resilient {
                            fit_into(frame, self.rect.w, self.rect.h)
                        } else {
                            fit(frame, self.rect.w, self.rect.h)
                        });
                    }
                } else if !self.resilient {
                    if let Ok(frame) = media::grab(self.rect) {
                        self.last = Some(frame);
                    }
                }
                let slot = geom::frame_slot(
                    self.clock.elapsed().as_nanos().saturating_sub(origin_ns),
                    fps,
                );
                if fate == SourceFate::Stop {
                    if slot > last_slot {
                        if let Some(frame) = self.last.as_ref() {
                            if let Err(error) = self.writer.push_at(frame, slot) {
                                break Err(error);
                            }
                        }
                    }
                    break Ok(());
                }
                if slot > last_slot {
                    if fate == SourceFate::Card && self.placeholder.is_none() {
                        self.placeholder = Some(no_signal(self.rect.w, self.rect.h));
                    }
                    let frame = if fate == SourceFate::Card {
                        self.placeholder.as_ref()
                    } else {
                        self.last.as_ref()
                    };
                    if let Some(frame) = frame {
                        if let Err(error) = self.writer.push_at(frame, slot) {
                            break Err(error);
                        }
                    }
                    last_slot = slot;
                }
            }
            if stop.load(Ordering::SeqCst) {
                break Ok(());
            }
            next_deadline += frame_time;
            let now = Instant::now();
            if next_deadline > now {
                std::thread::sleep(next_deadline - now);
            } else {
                next_deadline = now;
            }
        };
        let flush_result = self.pull_audio(&mut cursor, record_from, u64::MAX);
        self.inputs.clear();
        if let Some(source) = &mut self.video {
            source.stop();
        }
        let written = self.writer.finish();
        let written = loop_result.and(flush_result).and(written)?;
        if !self.animate {
            let keep = if options.audio_only {
                written.audio_packets > 0
            } else {
                written.video_packets > 0
            };
            if !keep {
                let _ = std::fs::remove_file(&self.scratch);
                return Err(if options.audio_only {
                    "Recording produced no audio".into()
                } else {
                    "Recording produced no video".into()
                });
            }
            publish_file(&self.scratch, &self.output)?;
        }
        Ok(Prepared {
            output: self.output.clone(),
            scratch: self.scratch.clone(),
            format: options.profile.format,
            fps: options.profile.video.fps,
            height: options.profile.video.height,
            animate: self.animate,
        })
    }
}

fn live_threads(requested: u32, width: u32, height: u32, fps: u32) -> u32 {
    if requested != 0 {
        return requested.max(1);
    }
    let cores = std::thread::available_parallelism()
        .map(|count| count.get() as u32)
        .unwrap_or(4);
    let heavy = u64::from(width) * u64::from(height) * u64::from(fps) > 1280 * 720 * 30;
    if heavy {
        cores.clamp(2, 8)
    } else {
        cores.clamp(1, 4)
    }
}

fn prepare(options: &RecordOptions, _stop: &AtomicBool) -> Result<Live, String> {
    let mut rect = options.rect.unwrap_or_else(media::cursor_monitor_rect);
    rect.w &= !1;
    rect.h &= !1;
    if !options.audio_only && (rect.w < 16 || rect.h < 16) {
        return Err("Capture area is too small".into());
    }
    let output = if options.audio_only {
        media::output_path("Audio", "mp3")?
    } else {
        media::output_path(
            if options.profile.format == Format::Mkv {
                "Video"
            } else {
                "Animation"
            },
            options.profile.format.extension(),
        )?
    };
    let animate = !options.audio_only && options.profile.format != Format::Mkv;
    let scratch = working_path(&output, animate);
    let audio_requested = (options.audio_only || options.profile.format == Format::Mkv)
        && (options.profile.audio.system || options.profile.audio.microphone);
    if options.audio_only && !audio_requested {
        return Err("Enable System audio or Microphone before capturing audio".into());
    }
    let clock = Instant::now();
    let mut inputs = Vec::new();
    if audio_requested && options.profile.audio.system {
        inputs.push(open_stream(true, None, clock)?);
    }
    if audio_requested && options.profile.audio.microphone {
        inputs.push(open_stream(
            false,
            microphone_name(options).as_deref(),
            clock,
        )?);
    }
    let mut recover = None;
    let mut video = if options.audio_only {
        None
    } else if options.window {
        let window = take_window().ok_or("The window is no longer available")?;
        let (capture, frame) = VideoCapture::start_window(window, options.profile.video.fps)?;
        rect.w = frame.width.max(2) & !1;
        rect.h = frame.height.max(2) & !1;
        recover = Some(window);
        Some(capture)
    } else {
        let (monitor, origin_x, origin_y, monitor_w, monitor_h) = monitor_of(rect);
        match VideoCapture::start(
            monitor,
            crop_for(rect, origin_x, origin_y, monitor_w, monitor_h),
            options.profile.video.fps,
        ) {
            Ok((capture, _)) => Some(capture),
            Err(error) => {
                if media::grab(rect).is_err() {
                    return Err(error);
                }
                None
            }
        }
    };
    let threads = live_threads(options.threads, rect.w, rect.h, options.profile.video.fps);
    let writer_rect = if options.audio_only {
        Rect {
            x: 0,
            y: 0,
            w: 16,
            h: 16,
        }
    } else {
        rect
    };
    let writer = match media::VideoWriter::new(
        &scratch,
        writer_rect,
        options.profile.video,
        options.profile.format,
        threads,
        !options.audio_only,
        audio_requested,
    ) {
        Ok(writer) => writer,
        Err(error) => {
            let _ = std::fs::remove_file(&scratch);
            return Err(error);
        }
    };
    let last = if options.audio_only {
        None
    } else {
        let raw = video
            .as_mut()
            .and_then(|source| source.latest())
            .or_else(|| media::grab(rect).ok())
            .unwrap_or(Frame {
                width: rect.w,
                height: rect.h,
                data: vec![0; rect.w as usize * rect.h as usize * 4],
            });
        // An odd window size is locked to the next even size. The stored frame must already
        // be that size, or the encoder slice panics and the release build aborts.
        Some(if options.window || options.resilient {
            fit_into(raw, rect.w, rect.h)
        } else {
            fit(raw, rect.w, rect.h)
        })
    };
    Ok(Live {
        rect,
        output,
        scratch,
        animate,
        clock,
        inputs,
        video,
        writer,
        last,
        recover,
        resilient: options.resilient || options.window,
        show_no_signal: options.show_no_signal,
        show_card: options.show_card,
        placeholder: None,
        next_retry: Instant::now(),
        was_usable: true,
    })
}

pub struct Session {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<Result<Prepared, String>>>,
    error: std::sync::mpsc::Receiver<String>,
}

impl Session {
    pub fn start(options: RecordOptions) -> Result<Self, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (error_tx, error_rx) = std::sync::mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("ezcap-capture".into())
            .spawn(move || {
                let result = record(options, worker_stop, ready_tx);
                if let Err(error) = &result {
                    let _ = error_tx.send(error.clone());
                }
                result
            })
            .map_err(|error| error.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(8)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
                error: error_rx,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                let _ = thread.join();
                Err("Capture did not start".into())
            }
        }
    }

    pub fn tick(&mut self) -> Result<(), String> {
        match self.error.try_recv() {
            Ok(error) => Err(error),
            Err(_) => Ok(()),
        }
    }

    pub fn finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| thread.is_finished())
    }

    pub fn stop(mut self) -> Result<Finalizer, String> {
        self.stop.store(true, Ordering::SeqCst);
        let prepared = self
            .thread
            .take()
            .ok_or("Capture is already stopping")?
            .join()
            .map_err(|_| "Capture thread stopped unexpectedly".to_string())??;
        if prepared.animate {
            let scratch = prepared.scratch;
            let output = prepared.output;
            let format = prepared.format;
            let fps = prepared.fps;
            let height = prepared.height;
            let worker = std::thread::Builder::new()
                .name("ezcap-encode".into())
                .spawn(move || {
                    media::convert_animation(&scratch, &output, format, fps, height)?;
                    let _ = std::fs::remove_file(&scratch);
                    Ok(output)
                })
                .map_err(|error| error.to_string())?;
            Ok(Finalizer {
                worker: Some(worker),
                ready: None,
            })
        } else {
            Ok(Finalizer {
                worker: None,
                ready: Some(Ok(prepared.output)),
            })
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub struct Finalizer {
    worker: Option<JoinHandle<Result<PathBuf, String>>>,
    ready: Option<Result<PathBuf, String>>,
}

#[cfg(test)]
mod source_card {
    use super::{
        fit, fit_into, no_signal, signal_present, source_fate, source_row, window_state_usable,
        SourceFate,
    };
    use crate::media::Frame;
    use crate::settings::Settings;

    struct SourceRecording {
        tracked: bool,
        show_no_signal: bool,
        show_card: bool,
        frames: Vec<SourceFate>,
        stopped: bool,
    }

    impl SourceRecording {
        fn tracked(show_no_signal: bool) -> Self {
            Self {
                tracked: true,
                show_no_signal,
                show_card: true,
                frames: Vec::new(),
                stopped: false,
            }
        }

        fn see(&mut self, available: bool) {
            if self.stopped {
                return;
            }
            match source_fate(self.tracked, available, self.show_no_signal, self.show_card) {
                SourceFate::Stop => self.stopped = true,
                fate => self.frames.push(fate),
            }
        }
    }

    #[test]
    fn no_signal_is_only_for_a_window_that_stopped_sending_frames() {
        assert!(
            signal_present(true, true, true, false),
            "a quiet window that is still open keeps its picture"
        );
        assert!(
            signal_present(true, false, true, true),
            "a frame that just arrived is a real signal"
        );
        assert!(
            !signal_present(true, false, true, false),
            "minimized or closed with no new frame"
        );
        assert!(!signal_present(true, true, false, false));
        assert!(signal_present(false, false, false, false));
        assert_eq!(
            source_fate(true, false, true, true),
            SourceFate::Card,
            "the card replaces the picture only when the signal is gone"
        );
        assert_eq!(source_fate(true, false, true, false), SourceFate::Hold);
        assert_eq!(source_fate(true, false, false, true), SourceFate::Stop);
        assert_eq!(source_fate(true, true, true, true), SourceFate::Live);
    }

    #[test]
    fn a_minimized_closed_or_hidden_window_is_not_usable() {
        assert!(!window_state_usable(
            true, true, true, false, false, 800, 600
        ));
        assert!(!window_state_usable(
            true, true, false, false, true, 800, 600
        ));
        assert!(!window_state_usable(
            false, false, false, false, false, 0, 0
        ));
        assert!(!window_state_usable(
            true, false, false, false, false, 800, 600
        ));
        assert!(window_state_usable(
            true, true, false, false, false, 800, 600
        ));
    }

    #[test]
    fn minimized_and_tiny_windows_are_not_offered_as_sources() {
        assert!(!source_row(true, false, 1200, 800, "Mail"));
        assert!(!source_row(false, true, 1200, 800, "Clock"));
        assert!(!source_row(false, false, 314, 50, "Clock"));
        assert!(!source_row(false, false, 314, 50, "Kimi"));
        assert!(!source_row(false, false, 800, 600, "ezcap"));
        assert!(source_row(false, false, 1280, 720, "Notes"));
    }

    #[test]
    fn an_odd_window_is_locked_to_an_even_frame_before_encoding() {
        let frame = Frame {
            width: 51,
            height: 40,
            data: vec![9, 8, 7, 255].repeat(51 * 40),
        };
        let fitted = fit(frame, 50, 40);
        assert_eq!((fitted.width, fitted.height), (50, 40));
        assert_eq!(fitted.data.len(), 50 * 40 * 4);
    }

    #[test]
    fn the_no_signal_card_is_on_by_default() {
        assert!(Settings::default().show_no_signal);
        assert!(Settings::default().show_card);
    }

    #[test]
    fn a_lost_source_keeps_recording_when_the_card_is_on() {
        let mut recording = SourceRecording::tracked(true);
        recording.see(true);
        recording.see(false);
        recording.see(false);
        recording.see(true);
        recording.see(false);
        assert!(!recording.stopped);
        assert_eq!(
            recording.frames,
            [
                SourceFate::Live,
                SourceFate::Card,
                SourceFate::Card,
                SourceFate::Live,
                SourceFate::Card,
            ]
        );
        let card = no_signal(320, 180);
        assert_eq!((card.width, card.height), (320, 180));
        assert!(card
            .data
            .chunks(4)
            .any(|pixel| pixel[0] > 20 || pixel[1] > 20 || pixel[2] > 20));
    }

    #[test]
    fn an_ordinary_capture_keeps_going_without_a_tracked_source() {
        let mut recording = SourceRecording {
            tracked: false,
            show_no_signal: false,
            show_card: true,
            frames: Vec::new(),
            stopped: false,
        };
        recording.see(false);
        recording.see(false);
        assert!(!recording.stopped);
        assert_eq!(recording.frames, [SourceFate::Live, SourceFate::Live]);
    }

    #[test]
    fn a_lost_source_keeps_the_last_frame_when_the_card_is_turned_off() {
        let mut recording = SourceRecording::tracked(true);
        recording.show_card = false;
        recording.see(true);
        recording.see(false);
        recording.see(true);
        assert!(!recording.stopped);
        assert_eq!(
            recording.frames,
            [SourceFate::Live, SourceFate::Hold, SourceFate::Live]
        );
    }

    #[test]
    fn a_lost_source_stops_recording_when_the_card_is_off() {
        let mut recording = SourceRecording::tracked(false);
        recording.see(true);
        recording.see(true);
        recording.see(false);
        recording.see(true);
        assert!(recording.stopped);
        assert_eq!(recording.frames, [SourceFate::Live, SourceFate::Live]);
    }

    #[test]
    fn the_no_signal_card_fills_the_locked_frame() {
        let frame = no_signal(320, 180);
        assert_eq!((frame.width, frame.height), (320, 180));
        assert!(frame
            .data
            .chunks(4)
            .any(|pixel| pixel[0] > 20 || pixel[1] > 20 || pixel[2] > 20));
    }

    #[test]
    fn a_resized_window_stays_inside_the_original_frame() {
        let source = Frame {
            width: 4,
            height: 2,
            data: vec![255, 0, 0, 255].repeat(8),
        };
        let fitted = fit_into(source, 8, 4);
        assert_eq!(
            (fitted.width, fitted.height, fitted.data.len()),
            (8, 4, 8 * 4 * 4)
        );
        assert!(fitted.data.chunks(4).any(|pixel| pixel[..3] == [0, 0, 0]));
        assert!(fitted.data.chunks(4).any(|pixel| pixel[0] == 255));
    }
}

impl Finalizer {
    pub fn step(&mut self) -> Result<Option<PathBuf>, String> {
        if let Some(result) = self.ready.take() {
            return result.map(Some);
        }
        let Some(worker) = &self.worker else {
            return Ok(None);
        };
        if !worker.is_finished() {
            return Ok(None);
        }
        let worker = self.worker.take().unwrap();
        Ok(Some(worker.join().map_err(|_| {
            "Encoder thread stopped unexpectedly".to_string()
        })??))
    }
}

static STAGED: Mutex<Option<Window>> = Mutex::new(None);

fn take_window() -> Option<Window> {
    STAGED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take()
}

pub enum Chosen {
    Monitor(Rect),
    Window(Rect),
}

fn window_usable(window: Window) -> bool {
    let hwnd = HWND(window.as_raw_hwnd());
    let (width, height) = window
        .rect()
        .map(|rect| (rect.right - rect.left, rect.bottom - rect.top))
        .unwrap_or((0, 0));
    window_state_usable(
        exists(hwnd),
        visible(hwnd),
        iconic(hwnd),
        cloaked(hwnd),
        minimized(hwnd),
        width,
        height,
    )
}

fn exists(hwnd: HWND) -> bool {
    unsafe {
        // SAFETY: The handle comes from the window being recorded. The call only checks that it still exists.
        IsWindow(hwnd)
    }
    .as_bool()
}

fn visible(hwnd: HWND) -> bool {
    unsafe {
        // SAFETY: The handle comes from the window being recorded. The call only reads its visibility.
        IsWindowVisible(hwnd)
    }
    .as_bool()
}

fn minimized(hwnd: HWND) -> bool {
    let mut place = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    let read = unsafe {
        // SAFETY: The handle comes from the window being recorded, and place is a writable WINDOWPLACEMENT with its size set.
        GetWindowPlacement(hwnd, &mut place)
    };
    if read.is_err() {
        return false;
    }
    let cmd = place.showCmd;
    cmd == SW_SHOWMINIMIZED.0 as u32
        || cmd == SW_MINIMIZE.0 as u32
        || cmd == SW_SHOWMINNOACTIVE.0 as u32
}

fn listed_window(window: &Window) -> bool {
    let title = window.title().unwrap_or_default();
    let (width, height) = window
        .rect()
        .map(|rect| (rect.right - rect.left, rect.bottom - rect.top))
        .unwrap_or((0, 0));
    let hwnd = HWND(window.as_raw_hwnd());
    source_row(iconic(hwnd), cloaked(hwnd), width, height, &title)
}

fn iconic(hwnd: HWND) -> bool {
    unsafe {
        // SAFETY: The handle comes from the window list or the window being recorded. The call only reads its state.
        IsIconic(hwnd)
    }
    .as_bool()
}

fn cloaked(hwnd: HWND) -> bool {
    let mut value = 0u32;
    let read = unsafe {
        // SAFETY: The handle is a window, and value is a writable u32 of the size passed in.
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            std::ptr::addr_of_mut!(value).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    read.is_ok() && value != 0
}

fn source_row(iconic: bool, cloaked: bool, width: i32, height: i32, title: &str) -> bool {
    if iconic || cloaked || width < 80 || height < 80 {
        return false;
    }
    let title = title.trim();
    !title.is_empty() && title != "ezcap" && !title.starts_with("ezcap ")
}

pub fn choose_source(owner: HWND) -> Option<Chosen> {
    let monitors = monitor_list();
    let windows = Window::enumerate().unwrap_or_default();
    let windows: Vec<Window> = windows.into_iter().filter(listed_window).collect();
    if monitors.is_empty() && windows.is_empty() {
        return None;
    }
    // SAFETY: The popup menu is destroyed before return. Item text buffers live until TrackPopupMenu returns.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    for (index, monitor) in monitors.iter().enumerate() {
        let label = wide(&format!(
            "Monitor {}  {}×{}",
            index + 1,
            monitor.w,
            monitor.h
        ));
        unsafe {
            // SAFETY: The menu was just created and the label is NUL-terminated.
            AppendMenuW(menu, MF_STRING, index as usize + 1, PCWSTR(label.as_ptr()))
        }
        .ok()?;
    }
    if !monitors.is_empty() && !windows.is_empty() {
        unsafe {
            // SAFETY: The menu was just created.
            AppendMenuW(menu, MF_SEPARATOR, 0, None)
        }
        .ok()?;
    }
    let mut labels = Vec::new();
    for window in &windows {
        let title = window.title().unwrap_or_else(|_| "Window".into());
        let (w, h) = window
            .rect()
            .map(|rect| {
                (
                    (rect.right - rect.left).max(0) as u32,
                    (rect.bottom - rect.top).max(0) as u32,
                )
            })
            .unwrap_or((0, 0));
        labels.push(wide(&format!("Window  {title}  {w}×{h}")));
    }
    for (index, label) in labels.iter().enumerate() {
        unsafe {
            // SAFETY: The menu was just created and the label is NUL-terminated.
            AppendMenuW(menu, MF_STRING, 1000 + index, PCWSTR(label.as_ptr()))
        }
        .ok()?;
    }
    let mut point = POINT::default();
    unsafe {
        // SAFETY: The POINT is a writable local.
        GetCursorPos(&mut point)
    }
    .ok()?;
    let id = unsafe {
        // SAFETY: The owner is the main window. TPM_RETURNCMD gives the chosen item id and does not send a command.
        TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            owner,
            None,
        )
    };
    unsafe {
        // SAFETY: The menu was created above and is not shown anymore.
        DestroyMenu(menu)
    }
    .ok()?;
    let id = id.0 as usize;
    if (1..=monitors.len()).contains(&id) {
        return Some(Chosen::Monitor(monitors[id - 1]));
    }
    let index = id.checked_sub(1000)?;
    let window = windows.get(index).copied()?;
    let rect = window.rect().ok()?;
    *STAGED.lock().unwrap_or_else(|error| error.into_inner()) = Some(window);
    Some(Chosen::Window(Rect {
        x: rect.left,
        y: rect.top,
        w: (rect.right - rect.left).max(0) as u32,
        h: (rect.bottom - rect.top).max(0) as u32,
    }))
}

pub fn grab_staged_window() -> Result<Frame, String> {
    let window = take_window().ok_or("The window is no longer available")?;
    let (mut capture, frame) = VideoCapture::start_window(window, 30)?;
    capture.stop();
    Ok(frame)
}

fn monitor_list() -> Vec<Rect> {
    // SAFETY: The callback's Vec stays alive until EnumDisplayMonitors returns.
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if unsafe {
            // SAFETY: The monitor handle came from the enumerator and cbSize is set.
            GetMonitorInfoW(monitor, &mut info)
        }
        .as_bool()
        {
            let rect = info.rcMonitor;
            unsafe {
                // SAFETY: data is the Vec passed into EnumDisplayMonitors and it outlives this callback.
                &mut *(data.0 as *mut Vec<Rect>)
            }
            .push(Rect {
                x: rect.left,
                y: rect.top,
                w: (rect.right - rect.left).max(0) as u32,
                h: (rect.bottom - rect.top).max(0) as u32,
            });
        }
        BOOL(1)
    }
    let mut rects = Vec::new();
    unsafe {
        // SAFETY: collect only writes into rects, which lives until this call returns.
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut rects as *mut Vec<Rect> as isize),
        )
    }
    .ok();
    rects
}

fn fit_into(frame: Frame, width: u32, height: u32) -> Frame {
    if frame.width == width && frame.height == height {
        return frame;
    }
    let scale =
        (width as f64 / frame.width.max(1) as f64).min(height as f64 / frame.height.max(1) as f64);
    let (sw, sh) = if scale < 1.0 {
        (
            ((frame.width as f64 * scale).round() as u32).clamp(1, width),
            ((frame.height as f64 * scale).round() as u32).clamp(1, height),
        )
    } else {
        (frame.width.min(width), frame.height.min(height))
    };
    let scaled = if sw == frame.width && sh == frame.height {
        frame
    } else {
        stretch(&frame, sw, sh)
    };
    let mut out = vec![0u8; width as usize * height as usize * 4];
    let x0 = (width - scaled.width) / 2;
    let y0 = (height - scaled.height) / 2;
    for row in 0..scaled.height as usize {
        let from = row * scaled.width as usize * 4;
        let to = ((y0 as usize + row) * width as usize + x0 as usize) * 4;
        out[to..to + scaled.width as usize * 4]
            .copy_from_slice(&scaled.data[from..from + scaled.width as usize * 4]);
    }
    Frame {
        width,
        height,
        data: out,
    }
}

fn stretch(frame: &Frame, width: u32, height: u32) -> Frame {
    let mut data = vec![0u8; width as usize * height as usize * 4];
    for y in 0..height {
        let sy = y as usize * frame.height as usize / height.max(1) as usize;
        for x in 0..width {
            let sx = x as usize * frame.width as usize / width.max(1) as usize;
            let from = (sy * frame.width as usize + sx) * 4;
            let to = (y as usize * width as usize + x as usize) * 4;
            data[to..to + 4].copy_from_slice(&frame.data[from..from + 4]);
        }
    }
    Frame {
        width,
        height,
        data,
    }
}

fn no_signal(width: u32, height: u32) -> Frame {
    let (source_w, source_h, pixels) = decoded_card();
    stretch(
        &Frame {
            width: *source_w,
            height: *source_h,
            data: pixels.clone(),
        },
        width,
        height,
    )
}

fn decoded_card() -> &'static (u32, u32, Vec<u8>) {
    static CARD: std::sync::OnceLock<(u32, u32, Vec<u8>)> = std::sync::OnceLock::new();
    CARD.get_or_init(|| {
        let decoder = png::Decoder::new(std::io::Cursor::new(include_bytes!(
            "../assets/no-signal.png"
        )));
        let mut reader = decoder.read_info().expect("no-signal.png");
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).expect("no-signal.png");
        buffer.truncate(info.buffer_size());
        let pixels = match info.color_type {
            png::ColorType::Rgba => buffer,
            png::ColorType::Rgb => buffer
                .chunks(3)
                .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
                .collect(),
            _ => buffer
                .chunks(4)
                .flat_map(|pixel| {
                    [
                        pixel[0],
                        pixel.get(1).copied().unwrap_or(0),
                        pixel.get(2).copied().unwrap_or(0),
                        255,
                    ]
                })
                .collect(),
        };
        (info.width, info.height, pixels)
    })
}
