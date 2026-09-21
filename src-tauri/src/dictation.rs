#[cfg(target_os = "windows")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModelInfo {
    pub id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub download_size: &'static str,
    pub memory: &'static str,
    pub recommended: bool,
    pub installed: bool,
}

struct SpeechModel {
    id: &'static str,
    title: &'static str,
    subtitle: &'static str,
    download_size: &'static str,
    memory: &'static str,
    recommended: bool,
    file: &'static str,
    url: &'static str,
}

const MODELS: &[SpeechModel] = &[
    SpeechModel {
        id: "tiny-en-q5",
        title: "Whisper Tiny · Fastest",
        subtitle: "For older laptops and short commands",
        download_size: "31 MB",
        memory: "~300 MB memory",
        recommended: false,
        file: "ggml-tiny.en-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q5_1.bin",
    },
    SpeechModel {
        id: "base-en-q5",
        title: "Whisper Base · Balanced",
        subtitle: "Fast, accurate dictation for most laptops",
        download_size: "57 MB",
        memory: "~400 MB memory",
        recommended: true,
        file: "ggml-base.en-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin",
    },
    SpeechModel {
        id: "small-en-q5",
        title: "Whisper Small · Accurate",
        subtitle: "Better names and longer notes; slower on older CPUs",
        download_size: "181 MB",
        memory: "~850 MB memory",
        recommended: false,
        file: "ggml-small.en-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en-q5_1.bin",
    },
];

fn model(id: &str) -> Option<&'static SpeechModel> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("models"))
        .map_err(|e| e.to_string())
}

pub fn model_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let m = model(id).ok_or_else(|| "Unknown speech model".to_string())?;
    Ok(models_dir(app)?.join(m.file))
}

pub fn list_models(app: &AppHandle) -> Result<Vec<SpeechModelInfo>, String> {
    let dir = models_dir(app)?;
    Ok(MODELS
        .iter()
        .map(|m| SpeechModelInfo {
            id: m.id,
            title: m.title,
            subtitle: m.subtitle,
            download_size: m.download_size,
            memory: m.memory,
            recommended: m.recommended,
            installed: dir.join(m.file).is_file(),
        })
        .collect())
}

pub fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    let m = model(&id).ok_or_else(|| "Unknown speech model".to_string())?;
    let dir = models_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let destination = dir.join(m.file);
    let partial = dir.join(format!("{}.part", m.file));

    let mut response = reqwest::blocking::Client::new()
        .get(m.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response.content_length().unwrap_or(0);
    let mut output = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut response, &mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut output, &buffer[..read]).map_err(|e| e.to_string())?;
        downloaded += read as u64;
        let percent = if total == 0 {
            0
        } else {
            downloaded.saturating_mul(100) / total
        };
        let _ = app.emit(
            "model-progress",
            serde_json::json!({ "id": id, "percent": percent }),
        );
    }
    std::fs::rename(&partial, &destination).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_model(app: &AppHandle, id: &str) -> Result<(), String> {
    let path = model_path(app, id)?;
    if path.is_file() {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationStatus {
    pub phase: String,
    pub level: f32,
    pub elapsed_ms: u128,
    pub message: String,
    pub model: String,
}

impl Default for DictationStatus {
    fn default() -> Self {
        Self {
            phase: "idle".into(),
            level: 0.0,
            elapsed_ms: 0,
            message: "Hold the shortcut and speak".into(),
            model: "BASE".into(),
        }
    }
}

pub struct DictationState {
    pub status: Arc<Mutex<DictationStatus>>,
    pub started: Mutex<Option<Instant>>,
    #[cfg(target_os = "windows")]
    pub recorder: Mutex<Option<Recorder>>,
}

impl Default for DictationState {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(DictationStatus::default())),
            started: Mutex::new(None),
            #[cfg(target_os = "windows")]
            recorder: Mutex::new(None),
        }
    }
}

pub fn status(state: &DictationState) -> DictationStatus {
    let mut value = state.status.lock().unwrap().clone();
    if value.phase == "recording" {
        if let Some(started) = *state.started.lock().unwrap() {
            value.elapsed_ms = started.elapsed().as_millis();
        }
    }
    value
}

fn show_bar(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("dictation") {
        win.show().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(app, "dictation", WebviewUrl::App("dictation.html".into()))
        .title("Capipaste Dictation")
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .transparent(true)
        .inner_size(620.0, 76.0)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(monitor) = app.primary_monitor().map_err(|e| e.to_string())? {
        let size = monitor.size();
        let pos = monitor.position();
        let x = pos.x + ((size.width as i32 - 620) / 2);
        let y = pos.y + (size.height as f32 * 0.80) as i32;
        win.set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn hide_bar_later(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1300));
        if let Some(win) = app.get_webview_window("dictation") {
            let _ = win.hide();
        }
        if let Some(state) = app.try_state::<DictationState>() {
            state.status.lock().unwrap().phase = "idle".into();
        }
    });
}

#[cfg(target_os = "windows")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[cfg(target_os = "windows")]
pub struct Recorder {
    stream: cpal::Stream,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: u32,
}

#[cfg(target_os = "windows")]
fn input_device(name: Option<&str>) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if let Some(wanted) = name.filter(|s| !s.is_empty()) {
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if device.name().ok().as_deref() == Some(wanted) {
                    return Ok(device);
                }
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| "No microphone found".into())
}

#[cfg(target_os = "windows")]
fn build_stream<T: cpal::SizedSample + Copy + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    status: Arc<Mutex<DictationStatus>>,
    convert: fn(T) -> f32,
) -> Result<cpal::Stream, String> {
    let channels = config.channels as usize;
    let mut ticks = 0_usize;
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                let mut mono = Vec::with_capacity(data.len() / channels.max(1));
                for frame in data.chunks(channels) {
                    let value =
                        frame.iter().copied().map(convert).sum::<f32>() / frame.len() as f32;
                    mono.push(value);
                }
                ticks += mono.len();
                if ticks >= 1600 {
                    ticks = 0;
                    let rms =
                        (mono.iter().map(|s| s * s).sum::<f32>() / mono.len().max(1) as f32).sqrt();
                    status.lock().unwrap().level = (rms * 7.5).clamp(0.02, 1.0);
                }
                samples.lock().unwrap().extend(mono);
            },
            move |e| eprintln!("microphone stream failed: {e}"),
            None,
        )
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn start_recorder(
    mic: Option<&str>,
    status: Arc<Mutex<DictationStatus>>,
) -> Result<Recorder, String> {
    let device = input_device(mic)?;
    let supported = device.default_input_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let sample_rate = supported.sample_rate().0;
    let config: cpal::StreamConfig = supported.into();
    let samples = Arc::new(Mutex::new(Vec::new()));
    let stream = match format {
        cpal::SampleFormat::F32 => {
            build_stream(&device, &config, samples.clone(), status, |v: f32| v)?
        }
        cpal::SampleFormat::I16 => {
            build_stream(&device, &config, samples.clone(), status, |v: i16| {
                v as f32 / 32768.0
            })?
        }
        cpal::SampleFormat::U16 => {
            build_stream(&device, &config, samples.clone(), status, |v: u16| {
                v as f32 / 32768.0 - 1.0
            })?
        }
        other => return Err(format!("Unsupported microphone format: {other}")),
    };
    stream.play().map_err(|e| e.to_string())?;
    Ok(Recorder {
        stream,
        samples,
        sample_rate,
    })
}

#[cfg(target_os = "windows")]
fn resample(input: &[f32], source_rate: u32) -> Vec<f32> {
    if source_rate == 16_000 || input.is_empty() {
        return input.to_vec();
    }
    let ratio = source_rate as f64 / 16_000.0;
    let output_len = (input.len() as f64 / ratio) as usize;
    (0..output_len)
        .map(|i| {
            let source = i as f64 * ratio;
            let left = source.floor() as usize;
            let right = (left + 1).min(input.len() - 1);
            let mix = (source - left as f64) as f32;
            input[left] * (1.0 - mix) + input[right] * mix
        })
        .collect()
}

#[cfg(target_os = "windows")]
fn transcribe(path: &Path, samples: &[f32], vocabulary: &str) -> Result<String, String> {
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
    let context = WhisperContext::new_with_params(path, WhisperContextParameters::default())
        .map_err(|e| format!("Could not load speech model: {e}"))?;
    let mut state = context.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("en"));
    params.set_translate(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    if !vocabulary.trim().is_empty() {
        params.set_initial_prompt(vocabulary);
    }
    state
        .full(params, samples)
        .map_err(|e| format!("Transcription failed: {e}"))?;
    let mut text = String::new();
    for segment in state.as_iter() {
        text.push_str(&segment.to_string());
    }
    Ok(text.trim().to_string())
}

pub fn clean_transcript(text: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for token in text.split_whitespace() {
        let word = token
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_ascii_lowercase();
        let filler = matches!(
            word.as_str(),
            "um" | "umm" | "uh" | "uhh" | "erm" | "ah" | "hmm"
        );
        if filler {
            continue;
        }
        let repeated = kept.last().is_some_and(|previous| {
            previous
                .trim_matches(|c: char| !c.is_alphabetic())
                .eq_ignore_ascii_case(&word)
        });
        if !repeated {
            kept.push(token);
        }
    }
    kept.join(" ")
}

pub fn start(app: &AppHandle, mic: Option<&str>, model_chip: &str) -> Result<(), String> {
    let state = app.state::<DictationState>();
    if state.status.lock().unwrap().phase != "idle" {
        return Ok(());
    }
    show_bar(app)?;
    {
        let mut value = state.status.lock().unwrap();
        value.phase = "recording".into();
        value.message = "Listening… release to finish".into();
        value.model = model_chip.into();
        value.level = 0.03;
        value.elapsed_ms = 0;
    }
    *state.started.lock().unwrap() = Some(Instant::now());
    #[cfg(target_os = "windows")]
    {
        match start_recorder(mic, state.status.clone()) {
            Ok(recorder) => *state.recorder.lock().unwrap() = Some(recorder),
            Err(e) => {
                let mut value = state.status.lock().unwrap();
                value.phase = "error".into();
                value.message = e.clone();
                hide_bar_later(app.clone());
                return Err(e);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = mic;
        let mut value = state.status.lock().unwrap();
        value.phase = "error".into();
        value.message = "Dictation is currently available in the Windows build".into();
    }
    Ok(())
}

pub fn finish(app: &AppHandle, model_id: &str, tidy: bool, vocabulary: &str) -> Result<(), String> {
    let state = app.state::<DictationState>();
    if state.status.lock().unwrap().phase != "recording" {
        return Ok(());
    }
    state.status.lock().unwrap().phase = "transcribing".into();
    state.status.lock().unwrap().message = "Transcribing on this PC…".into();

    #[cfg(target_os = "windows")]
    {
        let recorder = state
            .recorder
            .lock()
            .unwrap()
            .take()
            .ok_or("Microphone was not recording")?;
        let Recorder {
            stream,
            samples,
            sample_rate,
        } = recorder;
        drop(stream);
        let raw = samples.lock().unwrap().clone();
        let audio = resample(&raw, sample_rate);
        let path = model_path(app, model_id)?;
        if !path.is_file() {
            let message = "Download a speech model in Settings first";
            let mut value = state.status.lock().unwrap();
            value.phase = "error".into();
            value.message = message.into();
            hide_bar_later(app.clone());
            return Err(message.into());
        }
        let app = app.clone();
        let vocabulary = vocabulary.to_string();
        std::thread::spawn(move || {
            let outcome = transcribe(&path, &audio, &vocabulary).map(|text| {
                if tidy {
                    clean_transcript(&text)
                } else {
                    text
                }
            });
            match outcome {
                Ok(text) if !text.is_empty() => {
                    let copied = arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_text(text.clone()));
                    let state = app.state::<DictationState>();
                    let mut value = state.status.lock().unwrap();
                    if copied.is_ok() {
                        value.phase = "complete".into();
                        value.message = format!("Copied · {text}");
                    } else {
                        value.phase = "error".into();
                        value.message = "Transcribed, but the clipboard was unavailable".into();
                    }
                }
                Ok(_) => {
                    let state = app.state::<DictationState>();
                    let mut value = state.status.lock().unwrap();
                    value.phase = "error".into();
                    value.message = "No speech heard — try again".into();
                }
                Err(e) => {
                    let state = app.state::<DictationState>();
                    let mut value = state.status.lock().unwrap();
                    value.phase = "error".into();
                    value.message = e;
                }
            }
            hide_bar_later(app);
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (model_id, tidy, vocabulary);
        hide_bar_later(app.clone());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn microphones() -> Result<Vec<String>, String> {
    let host = cpal::default_host();
    let mut names = host
        .input_devices()
        .map_err(|e| e.to_string())?
        .filter_map(|device| device.name().ok())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

#[cfg(not(target_os = "windows"))]
pub fn microphones() -> Result<Vec<String>, String> {
    Ok(Vec::new())
}
