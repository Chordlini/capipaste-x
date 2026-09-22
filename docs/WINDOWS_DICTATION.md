# Windows dictation models and compute routing

Capipaste X selects a real accelerated runtime when the computer has one and labels the active backend in Settings. NVIDIA GPUs use CUDA, other compatible Windows GPUs use Vulkan, and machines without a compatible GPU keep the compact CPU Whisper fallback.

## Models available in the app

| Model | Download | Backend | Best for |
| --- | ---: | --- | --- |
| Nemotron 3.5 Streaming 0.6B Q8 | 707 MB | CUDA / Vulkan / CPU | Recommended low-latency multilingual dictation |
| Nemotron Speech Streaming English 0.6B Q8 | 668 MB | CUDA / Vulkan / CPU | Fast English-only dictation |
| Parakeet TDT 0.6B v3 Q8 | 681 MB | CUDA / Vulkan / CPU | High-throughput completed recordings |
| Whisper Tiny / Base / Small English Q5 | 31 / 57 / 181 MB | CPU | Compact fallback for older laptops |

The Nemotron and Parakeet models run through NVIDIA's official [NeMo-Speech.cpp](https://github.com/NVIDIA/NeMo-Speech.cpp) runtime. Capipaste downloads the matching release runtime, verifies both the runtime and model SHA-256 checksums, and keeps the selected ASR model warm between dictations. The local server is bound only to `127.0.0.1`.

Nemotron 3.5 is a cache-aware FastConformer-RNNT streaming model with native punctuation and capitalization. Its supported chunk sizes start at 80 ms. NVIDIA recommends the separate English Nemotron checkpoint for English-only use; the 3.5 checkpoint is the flexible multilingual choice. Parakeet TDT v3 is exposed for users who value throughput, but it is not presented as the lowest-latency option.

## Product behavior

- Hold Right Alt to record; release it to transcribe, paste into the focused app, and retain a clipboard copy.
- A 64-pixel floating waveform pill mirrors the Mac app's timer, dot matrix, status, and model chip.
- Settings shows the detected device and `CUDA`, `VULKAN`, or `CPU` explicitly.
- Model inference, punctuation, cleanup, clipboard handling, and paste stay local. Network access is used only to download a selected model and its runtime.
- The runtime and models are stored in the Capipaste application-data directory rather than inflating every installer.

## RTX 5080 validation

On the development RTX 5080, the official NeMo-Speech.cpp CUDA runtime detected the GPU and transcribed a 9.5-second synthetic dictation in 60 ms with the Nemotron 3.5 Q8 model after warmup. The first request after server startup took 629 ms. The previous CPU Whisper path took about 15 seconds in the user's real dictation test.

## Next latency step

The persistent local server removes model-load time and makes release-to-paste fast. The runtime also exposes a realtime PCM WebSocket and stable native streaming API; feeding microphone chunks during the key hold is the next step if measurements on slower GPUs show that post-release inference is still perceptible.
