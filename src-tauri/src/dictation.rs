use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::{Child, Command, Stdio};
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
    pub engine: &'static str,
    pub compute: String,
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
    sha256: &'static str,
    engine: &'static str,
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
        sha256: "",
        engine: "whisper",
    },
    SpeechModel {
        id: "base-en-q5",
        title: "Whisper Base · Balanced",
        subtitle: "Fast, accurate dictation for most laptops",
        download_size: "57 MB",
        memory: "~400 MB memory",
        recommended: false,
        file: "ggml-base.en-q5_1.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en-q5_1.bin",
        sha256: "",
        engine: "whisper",
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
        sha256: "",
        engine: "whisper",
    },
    SpeechModel {
        id: "nemotron-3.5-q8",
        title: "Nemotron 3.5 Streaming · Multilingual",
        subtitle: "Low-latency streaming with native punctuation and 32 ready-to-use locales",
        download_size: "707 MB + runtime",
        memory: "GPU recommended · ~1.5 GB VRAM",
        recommended: false,
        file: "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf",
        url: "https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/resolve/1c8deaecc64b91f034d73e08dd8b64625eb3395d/nemotron-3.5-asr-streaming-0.6b.q8_0.gguf",
        sha256: "a5c435f294eea8f88ce68dd27b8c3bfea7f777cb2fbba04fcd30eaa555f429ae",
        engine: "nemo",
    },
    SpeechModel {
        id: "nemotron-en-q8",
        title: "Nemotron Streaming · English",
        subtitle: "Fastest Nemotron choice for English-only dictation",
        download_size: "668 MB + runtime",
        memory: "GPU recommended · ~1.4 GB VRAM",
        recommended: false,
        file: "nemotron-speech-streaming-en-0.6b.q8_0.gguf",
        url: "https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b/resolve/ebe59e5a817142986528bbbee5dba8db7b38ed50/nemotron-speech-streaming-en-0.6b.q8_0.gguf",
        sha256: "d9a01898d2a611c8764e23a1c2f45e70bbd5a425dc4de93692ac951dd603812d",
        engine: "nemo",
    },
    SpeechModel {
        id: "parakeet-tdt-q8",
        title: "Parakeet TDT 0.6B v3 · Accurate",
        subtitle: "Best English accuracy in this local model set, with punctuation and capitalization",
        download_size: "681 MB + runtime",
        memory: "GPU recommended · ~1.5 GB VRAM",
        recommended: true,
        file: "parakeet-tdt-0.6b-v3.q8_0.gguf",
        url: "https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3/resolve/541d1f99c6b0c3cd0b11a95167540bb8edefd82b/parakeet-tdt-0.6b-v3.q8_0.gguf",
        sha256: "e3880d0aaaaf2c308ea2c35016b2b895c423eb3fda924c1b463d1c19b7f4d32e",
        engine: "nemo",
    },
];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputeInfo {
    pub backend: String,
    pub device: String,
    pub accelerated: bool,
    pub detail: String,
}

#[cfg(target_os = "windows")]
pub fn compute_info() -> ComputeInfo {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    if let Ok(output) = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        let name = String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        if output.status.success() && !name.is_empty() {
            return ComputeInfo {
                backend: "CUDA".into(),
                device: name.clone(),
                accelerated: true,
                detail: format!("{name} · NVIDIA CUDA"),
            };
        }
    }
    if Path::new(r"C:\Windows\System32\vulkan-1.dll").is_file() {
        return ComputeInfo {
            backend: "VULKAN".into(),
            device: "Windows GPU".into(),
            accelerated: true,
            detail: "Compatible GPU · Vulkan".into(),
        };
    }
    ComputeInfo {
        backend: "CPU".into(),
        device: "Processor".into(),
        accelerated: false,
        detail: "No compatible GPU runtime detected · CPU fallback".into(),
    }
}

#[cfg(not(target_os = "windows"))]
pub fn compute_info() -> ComputeInfo {
    ComputeInfo {
        backend: "CPU".into(),
        device: "Processor".into(),
        accelerated: false,
        detail: "CPU fallback".into(),
    }
}

fn model(id: &str) -> Option<&'static SpeechModel> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| p.join("models"))
        .map_err(|e| e.to_string())
}

fn runtime_dir(app: &AppHandle, backend: &str) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|p| {
            p.join("nemo-speech-0.1.0")
                .join(backend.to_ascii_lowercase())
        })
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn runtime_spec(backend: &str) -> (&'static str, &'static str) {
    match backend {
        "CUDA" => (
            "https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/v0.1.0/nemo-speech-0.1.0-windows-x86_64-cuda.zip",
            "ba024204e76ca2fa4eefa8787506c3c49e418147f627f60cf9206a582b60089c",
        ),
        "VULKAN" => (
            "https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/v0.1.0/nemo-speech-0.1.0-windows-x86_64-vulkan.zip",
            "b5e7b04a637da4eb25a60253e2db65774998e8dfb48c08b4db763009b82ac7ac",
        ),
        _ => (
            "https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/v0.1.0/nemo-speech-0.1.0-windows-x86_64-cpu.zip",
            "5e4ea81046012edcd77fd8848de8eefb5a4ba38cc26f52eb544ab184695a75d6",
        ),
    }
}

fn sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn download_file(
    app: &AppHandle,
    id: &str,
    url: &str,
    partial: &Path,
    from_percent: u64,
    span_percent: u64,
) -> Result<(), String> {
    let mut response = reqwest::blocking::Client::new()
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response.content_length().unwrap_or(0);
    let mut output = std::fs::File::create(partial).map_err(|e| e.to_string())?;
    let mut downloaded = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = std::io::Read::read(&mut response, &mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut output, &buffer[..read]).map_err(|e| e.to_string())?;
        downloaded += read as u64;
        let fraction = if total == 0 {
            0
        } else {
            downloaded.saturating_mul(span_percent) / total
        };
        let _ = app.emit(
            "model-progress",
            serde_json::json!({ "id": id, "percent": from_percent + fraction }),
        );
    }
    output.sync_all().map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn ensure_nemo_runtime(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let backend = compute_info().backend;
    let dir = runtime_dir(app, &backend)?;
    let exe = dir.join("bin").join("nemo-speech.exe");
    if exe.is_file() {
        return Ok(exe);
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (url, expected) = runtime_spec(&backend);
    let archive = dir.with_extension("zip.part");
    download_file(app, id, url, &archive, 0, 15)?;
    let actual = sha256(&archive)?;
    if actual != expected {
        let _ = std::fs::remove_file(&archive);
        return Err("NeMo runtime download did not pass its checksum".into());
    }
    let file = std::fs::File::open(&archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        let target = dir.join(name);
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut output = std::fs::File::create(&target).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut output).map_err(|e| e.to_string())?;
        }
    }
    let _ = std::fs::remove_file(&archive);
    if exe.is_file() {
        Ok(exe)
    } else {
        Err("NeMo runtime archive was incomplete".into())
    }
}

pub fn model_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    let m = model(id).ok_or_else(|| "Unknown speech model".to_string())?;
    Ok(models_dir(app)?.join(m.file))
}

pub fn list_models(app: &AppHandle) -> Result<Vec<SpeechModelInfo>, String> {
    let dir = models_dir(app)?;
    let compute = compute_info();
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
            engine: m.engine,
            compute: if m.engine == "nemo" {
                compute.backend.clone()
            } else {
                "CPU".into()
            },
        })
        .collect())
}

pub fn download_model(app: AppHandle, id: String) -> Result<(), String> {
    let m = model(&id).ok_or_else(|| "Unknown speech model".to_string())?;
    let dir = models_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let destination = dir.join(m.file);
    let partial = dir.join(format!("{}.part", m.file));
    let start = if m.engine == "nemo" {
        #[cfg(target_os = "windows")]
        {
            ensure_nemo_runtime(&app, &id)?;
        }
        15
    } else {
        0
    };
    download_file(&app, &id, m.url, &partial, start, 100 - start)?;
    if !m.sha256.is_empty() && sha256(&partial)? != m.sha256 {
        let _ = std::fs::remove_file(&partial);
        return Err("Speech model download did not pass its checksum".into());
    }
    std::fs::rename(&partial, &destination).map_err(|e| e.to_string())?;
    let _ = app.emit(
        "model-progress",
        serde_json::json!({ "id": id, "percent": 100 }),
    );
    Ok(())
}

pub fn delete_model(app: &AppHandle, id: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let state = app.state::<DictationState>();
        let mut server = state.nemo_server.lock().unwrap();
        if server
            .as_ref()
            .is_some_and(|running| running.model_id == id)
        {
            *server = None;
        }
    }
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
    #[cfg(target_os = "windows")]
    nemo_server: Mutex<Option<NemoServer>>,
}

impl Default for DictationState {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(DictationStatus::default())),
            started: Mutex::new(None),
            #[cfg(target_os = "windows")]
            recorder: Mutex::new(None),
            #[cfg(target_os = "windows")]
            nemo_server: Mutex::new(None),
        }
    }
}

#[cfg(target_os = "windows")]
struct NemoServer {
    model_id: String,
    child: Child,
}

#[cfg(target_os = "windows")]
impl Drop for NemoServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(target_os = "windows")]
fn nemo_device(backend: &str) -> &'static str {
    match backend {
        "CUDA" => "cuda:0",
        "VULKAN" => "vulkan:0",
        _ => "cpu",
    }
}

#[cfg(target_os = "windows")]
fn prime_nemo() -> Result<(), String> {
    let samples = 5_120_u32;
    let data_bytes = samples * 2;
    let mut wav = Vec::with_capacity((44 + data_bytes) as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&16_000_u32.to_le_bytes());
    wav.extend_from_slice(&32_000_u32.to_le_bytes());
    wav.extend_from_slice(&2_u16.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_bytes.to_le_bytes());
    wav.resize((44 + data_bytes) as usize, 0);
    let form = reqwest::blocking::multipart::Form::new()
        .part(
            "file",
            reqwest::blocking::multipart::Part::bytes(wav).file_name("warmup.wav"),
        )
        .text("model", "default")
        .text("language", "en-US");
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?
        .post("http://127.0.0.1:49327/v1/audio/transcriptions")
        .multipart(form)
        .send()
        .and_then(|response| response.error_for_status())
        .map(|_| ())
        .map_err(|e| format!("Could not prime the GPU speech model: {e}"))
}

#[cfg(target_os = "windows")]
pub fn prepare_model(app: &AppHandle, model_id: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let selected = model(model_id).ok_or_else(|| "Unknown speech model".to_string())?;
    if selected.engine != "nemo" {
        return Ok(());
    }
    let path = model_path(app, model_id)?;
    if !path.is_file() {
        return Err("Download the selected speech model in Settings first".into());
    }
    let backend = compute_info().backend;
    let exe = ensure_nemo_runtime(app, model_id)?;
    let state = app.state::<DictationState>();
    let mut slot = state.nemo_server.lock().unwrap();
    if let Some(server) = slot.as_mut() {
        if server.model_id == model_id
            && server
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_none()
        {
            return Ok(());
        }
    }
    *slot = None;
    let bin = exe.parent().ok_or("NeMo runtime path is invalid")?;
    let child = Command::new(&exe)
        .args([
            "serve",
            "--host",
            "127.0.0.1",
            "--port",
            "49327",
            "--threads",
            "2",
            "--no-ui",
            "--asr-model",
        ])
        .arg(&path)
        .args(["--device", nemo_device(&backend)])
        .current_dir(bin)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Could not start the NeMo speech runtime: {e}"))?;
    *slot = Some(NemoServer {
        model_id: model_id.into(),
        child,
    });
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Ok(response) = client.get("http://127.0.0.1:49327/ready").send() {
            if response.status().is_success() {
                if let Err(problem) = prime_nemo() {
                    *slot = None;
                    return Err(problem);
                }
                return Ok(());
            }
        }
        if let Some(server) = slot.as_mut() {
            if let Some(exit) = server.child.try_wait().map_err(|e| e.to_string())? {
                *slot = None;
                return Err(format!(
                    "NeMo speech runtime stopped during GPU warmup ({exit})"
                ));
            }
        }
        if Instant::now() >= deadline {
            *slot = None;
            return Err("NeMo speech runtime did not become ready in 90 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(120));
    }
}

#[cfg(not(target_os = "windows"))]
pub fn prepare_model(_app: &AppHandle, _model_id: &str) -> Result<(), String> {
    Ok(())
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
        .focusable(false)
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

pub fn show_error(app: &AppHandle, message: &str) {
    let _ = show_bar(app);
    let state = app.state::<DictationState>();
    let mut value = state.status.lock().unwrap();
    value.phase = "error".into();
    value.message = message.into();
    drop(value);
    hide_bar_later(app.clone());
}

#[cfg(target_os = "windows")]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

#[cfg(target_os = "windows")]
pub struct Recorder {
    stop: std::sync::mpsc::Sender<()>,
    worker: std::thread::JoinHandle<()>,
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
    let samples = Arc::new(Mutex::new(Vec::new()));
    let thread_samples = samples.clone();
    let mic = mic.map(str::to_owned);
    let (stop, stop_rx) = std::sync::mpsc::channel();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    // CPAL streams are intentionally !Send. Own the stream for its full lifetime
    // on this dedicated audio thread and keep only Send handles in Tauri state.
    let worker = std::thread::spawn(move || {
        let setup = (|| -> Result<(cpal::Stream, u32), String> {
            let device = input_device(mic.as_deref())?;
            let supported = device.default_input_config().map_err(|e| e.to_string())?;
            let format = supported.sample_format();
            let sample_rate = supported.sample_rate().0;
            let config: cpal::StreamConfig = supported.into();
            let stream = match format {
                cpal::SampleFormat::F32 => build_stream(
                    &device,
                    &config,
                    thread_samples.clone(),
                    status.clone(),
                    |v: f32| v,
                )?,
                cpal::SampleFormat::I16 => build_stream(
                    &device,
                    &config,
                    thread_samples.clone(),
                    status.clone(),
                    |v: i16| v as f32 / 32768.0,
                )?,
                cpal::SampleFormat::U16 => build_stream(
                    &device,
                    &config,
                    thread_samples.clone(),
                    status.clone(),
                    |v: u16| v as f32 / 32768.0 - 1.0,
                )?,
                other => return Err(format!("Unsupported microphone format: {other}")),
            };
            stream.play().map_err(|e| e.to_string())?;
            Ok((stream, sample_rate))
        })();
        match setup {
            Ok((stream, sample_rate)) => {
                let _ = ready_tx.send(Ok(sample_rate));
                let _ = stop_rx.recv();
                drop(stream);
            }
            Err(problem) => {
                let _ = ready_tx.send(Err(problem));
            }
        }
    });
    let sample_rate = ready_rx
        .recv()
        .map_err(|_| "Microphone thread stopped unexpectedly".to_string())??;
    Ok(Recorder {
        stop,
        worker,
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

#[cfg(target_os = "windows")]
fn transcribe_nemo(samples: &[f32], vocabulary: &str) -> Result<String, String> {
    let temp = std::env::temp_dir().join(format!(
        "capipaste-{}-{}.wav",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis()
    ));
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&temp, spec).map_err(|e| e.to_string())?;
    for sample in samples {
        writer
            .write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    let part = reqwest::blocking::multipart::Part::file(&temp)
        .map_err(|e| e.to_string())?
        .file_name("dictation.wav");
    let mut form = reqwest::blocking::multipart::Form::new()
        .part("file", part)
        .text("model", "default")
        .text("language", "en-US")
        .text("response_format", "json")
        .text("automatic_punctuation", "true");
    let prompt = vocabulary
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !prompt.is_empty() {
        form = form.text("prompt", prompt);
    }
    let response = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?
        .post("http://127.0.0.1:49327/v1/audio/transcriptions")
        .multipart(form)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("GPU transcription failed: {e}"));
    let _ = std::fs::remove_file(&temp);
    let value: serde_json::Value = response?
        .json()
        .map_err(|e| format!("Speech runtime returned invalid output: {e}"))?;
    value
        .get("text")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .map(str::to_string)
        .ok_or_else(|| "Speech runtime returned no transcript".into())
}

#[cfg(target_os = "windows")]
fn paste_clipboard() -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
    };

    fn key(vk: u16, flags: u32) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    // Give the physical Right Alt release time to clear before synthesizing
    // Ctrl+V into the app that retained focus behind the non-focusable pill.
    std::thread::sleep(Duration::from_millis(35));
    let inputs = [
        key(VK_CONTROL, 0),
        key(VK_V, 0),
        key(VK_V, KEYEVENTF_KEYUP),
        key(VK_CONTROL, KEYEVENTF_KEYUP),
    ];
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err("Windows blocked automatic paste".into())
    }
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
    let selected = model(model_id).ok_or_else(|| "Unknown speech model".to_string())?;
    let backend = if selected.engine == "nemo" {
        compute_info().backend
    } else {
        "CPU".into()
    };
    state.status.lock().unwrap().message = format!("Finishing on {backend}…");

    #[cfg(target_os = "windows")]
    {
        let recorder = state
            .recorder
            .lock()
            .unwrap()
            .take()
            .ok_or("Microphone was not recording")?;
        let Recorder {
            stop,
            worker,
            samples,
            sample_rate,
        } = recorder;
        let _ = stop.send(());
        let _ = worker.join();
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
        let engine = selected.engine;
        std::thread::spawn(move || {
            let started = Instant::now();
            let outcome = if engine == "nemo" {
                transcribe_nemo(&audio, &vocabulary)
            } else {
                transcribe(&path, &audio, &vocabulary)
            }
            .map(|text| if tidy { clean_transcript(&text) } else { text });
            match outcome {
                Ok(text) if !text.is_empty() => {
                    let copied = arboard::Clipboard::new()
                        .and_then(|mut clipboard| clipboard.set_text(text.clone()));
                    #[cfg(target_os = "windows")]
                    let pasted = copied.is_ok() && paste_clipboard().is_ok();
                    let state = app.state::<DictationState>();
                    let mut value = state.status.lock().unwrap();
                    if copied.is_ok() {
                        value.phase = "complete".into();
                        #[cfg(target_os = "windows")]
                        {
                            value.message = if pasted {
                                format!(
                                    "Pasted in {:.1}s · {text}",
                                    started.elapsed().as_secs_f32()
                                )
                            } else {
                                format!(
                                    "Copied in {:.1}s · {text}",
                                    started.elapsed().as_secs_f32()
                                )
                            };
                        }
                        #[cfg(not(target_os = "windows"))]
                        {
                            value.message = format!("Copied · {text}");
                        }
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
