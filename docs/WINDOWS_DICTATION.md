# Windows dictation model plan

Capipaste X uses a CPU-first stack so dictation remains private, responsive, and useful on ordinary Windows laptops without a discrete GPU.

## Speech models shipped in the app

| Model | Download | Typical memory | Best for |
| --- | ---: | ---: | --- |
| Whisper Tiny English Q5 | 31 MB | ~300 MB | Older laptops and short commands |
| Whisper Base English Q5 | 57 MB | ~400 MB | Default for most laptops |
| Whisper Small English Q5 | 181 MB | ~850 MB | Better accuracy on recent Core i5/Ryzen 5-class hardware |

The app runs these GGML models through [whisper.cpp](https://github.com/ggml-org/whisper.cpp), which supports CPU-only Windows inference, x86 AVX, Vulkan, CUDA, OpenVINO, and integer quantization. Whisper supplies capitalization and punctuation. Capipaste's lightweight tidy pass removes standalone filler sounds and immediately repeated words without another model or more memory.

## Models evaluated

- [Moonshine Streaming](https://github.com/moonshine-ai/moonshine) is the strongest candidate for a future low-latency engine. Its small streaming checkpoints and on-device focus are attractive, but Whisper.cpp currently has the simpler, more mature Windows packaging path.
- [NVIDIA Parakeet TDT-CTC 110M](https://huggingface.co/nvidia/parakeet-tdt_ctc-110m) is fast, accurate, and emits punctuation and capitalization. Its official NeMo/PyTorch deployment is heavier than the runtime we want to ship to a broad laptop audience.
- [Qwen 3.5 0.8B](https://huggingface.co/Qwen/Qwen3.5-0.8B) is a candidate for optional false-start and self-correction cleanup. The Mac app's Qwen 3.5 2B path remains the higher-quality target. Both should stay optional because they add substantially more download size and memory than the lightweight cleanup shipped now.

## Product behavior

- Hold the Dictate shortcut to record; release it to transcribe.
- A 64-pixel floating waveform pill mirrors the Mac app's timer, dot matrix, status, and model chip.
- Text is copied when transcription finishes. Audio is not uploaded or retained.
- Capture and Dictate shortcuts, microphone, speech model, cleanup, and vocabulary are editable in Settings.

## Follow-up work

1. Add an opt-in Qwen tidy download for semantic false-start and correction handling.
2. Add automatic paste behind an explicit Windows Accessibility-style permission/on-off setting.
3. Benchmark Moonshine Streaming on Intel, AMD, and ARM64 Windows hardware before exposing it as an engine choice.
