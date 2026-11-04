//! Screen and audio capture.
//!
//! Windows Graphics Capture and WASAPI streams exist only while a recording
//! or a still grab is in progress. Dropping the session stops both, so an
//! idle tray process keeps no capture device, no audio client and no extra frame.

use crate::geom::{self, AudioQueue};
use crate::media::{self, Format, Frame, RecordOptions, Rect};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows_capture::capture::{CaptureControl, Context, GraphicsCaptureApiHandler};
use windows_capture::frame::Frame as WgcFrame;
use windows_capture::graphics_capture_api::InternalCaptureControl;
use windows_capture::monitor::Monitor;
use windows_capture::settings::{
    ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
    MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
};

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
        };
        let frame = wait_frame(&slot, Duration::from_secs(3))?;
        Ok((capture, frame))
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
}

impl Live {
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
                if let Some(source) = &mut self.video {
                    if let Some(frame) = source.latest() {
                        self.last = Some(fit(frame, self.rect.w, self.rect.h));
                    }
                } else if let Ok(frame) = media::grab(self.rect) {
                    self.last = Some(frame);
                }
                let slot = geom::frame_slot(
                    self.clock.elapsed().as_nanos().saturating_sub(origin_ns),
                    fps,
                );
                if slot > last_slot {
                    if let Some(frame) = &self.last {
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
    let mut video = if options.audio_only {
        None
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
        Some(
            video
                .as_mut()
                .and_then(|source| source.latest())
                .or_else(|| media::grab(rect).ok())
                .unwrap_or(Frame {
                    width: rect.w,
                    height: rect.h,
                    data: vec![0; rect.w as usize * rect.h as usize * 4],
                }),
        )
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
