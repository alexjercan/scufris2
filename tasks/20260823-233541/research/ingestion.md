# Multimodal ingestion research: video, audio, PDF, image

Scope: turn web media into library-ready learning material. Flagship case:
video becomes a word:timestamp transcript plus relevant extracted frames, so
content is consumable as text and images without replaying the video, and
later compiles into interactive HTML courses. Local-first; a cloud model may
be justified per modality only with the tradeoff stated. Everything must
package on NixOS.

Verified local baseline (2026-08-24), from
`~/personal/nix.dotfiles/home/modules/agents/pi-extensions/voice-stt/module.nix`:
a systemd user service `whisper-server` runs `whisper-cpp-vulkan`'s
`whisper-server` binary bound to `127.0.0.1:10301`, inference path
`/inference`, language `auto`, model `ggml-large-v3-turbo-q5_0.bin`
(fetched from `ggerganov/whisper.cpp` on Hugging Face). Pi's `voice-stt`
extension talks to it as an OpenAI-compatible endpoint
(`http://127.0.0.1:10301/inference`, `model: whisper-1`). Host GPU is NVIDIA,
driven through the Vulkan backend (`hardware.nvidia` set, no CUDA toolkit
wired into whisper's package selection) - `nix/voice.nix` in this repo shows
the sibling Piper TTS packaging pattern (fetch model + config as separate
derivations, symlink into a `share/scufris/voices` output), which is the
template to follow for any ingestion model assets.

## 1. Timestamped transcription

### 1.1 whisper.cpp word-level timestamps

whisper.cpp gets token-level timestamps from the model's cross-attention
weights via Dynamic Time Warping (DTW): DTW aligns text tokens to audio
frames, giving each token a start/end time; adjacent sub-word tokens are then
merged into whole words. This is a native, no-extra-model technique, but it
is not the same as forced alignment - the underlying whisper.cpp
maintainers themselves state the approach "lacks robustness" because Whisper
was not trained to place a timestamp after every word, only every so often
(commonly sentence boundaries), so token/word timestamps can be locally
noisy even though segment timestamps are reliable
([discussion #314](https://github.com/ggml-org/whisper.cpp/discussions/314)).

The CLI (`examples/main`) exposes this through `-ml`/`--max-len N`
(max segment length in characters; `-ml 1` forces one word/token per
segment), `-sow`/`--split-on-word` (split on word boundaries, not raw
token boundaries), and `-wt`/`--word-thold` (word timestamp probability
threshold). Output formats include plain text, `-ovtt` (WebVTT), `-osrt`
(SRT), `-ocsv` (CSV per segment/word), and `-oj` (JSON).

**whisper-server (already running locally) supports this over HTTP.** The
`/inference` multipart endpoint (`examples/server/server.cpp`) reads the same
knobs as request fields: `response_format` (`json` | `text` | `srt` |
`verbose_json` | `vtt`), `max_len`, `split_on_word`, `word_thold`,
`token_timestamps`, plus `temperature`, `beam_size`, `best_of`, `language`,
`translate`, `vad*` fields, etc. When `response_format=verbose_json`, each
segment includes a `words` array with `{word, start, end, t_dtw,
probability}` per word
([server README](https://github.com/ggml-org/whisper.cpp/blob/master/examples/server/README.md),
[server.cpp](https://github.com/ggml-org/whisper.cpp/blob/master/examples/server/server.cpp)).
This means **no new service is needed for word:timestamp transcripts** - the
existing `whisper-server` on `127.0.0.1:10301` already produces them; the
ingestion pipeline only has to call `/inference` with
`response_format=verbose_json` (and probably `token_timestamps=true`,
`split_on_word=true`) instead of the OpenAI-compatible shim Pi's `voice-stt`
extension uses. This is a distinct call path from `voice-stt`'s existing use
of the endpoint, so it does not disturb the STT extension.

Caveat: whisper-server currently runs `large-v3-turbo` tuned for low-latency
interactive STT (short utterances, `--language auto`). Long video files (tens
of minutes) sent as one upload will work but tie up the single-mutex server
for the duration and block interactive STT requests during that window;
batch ingestion should either run against a second instance/model invocation
or be scheduled when the desktop is not in an active voice conversation. The
server's own `-mc/--max-context`, VAD options, and the fact that it holds one
global `whisper_context` mutex mean it is not designed for concurrent
interactive-and-batch use.

### 1.2 faster-whisper

CTranslate2-based re-implementation; word timestamps come from the same
cross-attention/DTW technique whisper.cpp uses (`word_timestamps=True`), not
forced alignment. Packaged in nixpkgs as
`python313Packages.faster-whisper` (1.2.1) and as a ready server,
`wyoming-faster-whisper` (3.5.0). Useful if a Python-native batch pipeline
is preferred over shelling to `whisper-server`, but it adds a second
inference engine and a second model download path next to the whisper.cpp
model already pinned for STT - duplicate maintenance for the same DTW-based
accuracy tier. Not recommended as the primary path when whisper-server
already exists.

### 1.3 WhisperX (word alignment quality)

WhisperX runs faster-whisper for transcription, then a **separate forced
alignment** pass with a wav2vec2 CTC model per language, plus VAD-based
chunking and optional diarization. This buys materially tighter word
timestamps: **approximately +-50 ms with wav2vec2 forced alignment vs.
roughly +-500 ms from vanilla Whisper/whisper.cpp DTW timestamps**
([m-bain/whisperX](https://github.com/m-bain/whisperx),
[Modal: choosing Whisper variants](https://modal.com/blog/choosing-whisper-variants)).
Known weakness: wav2vec2 alignment degrades more than Whisper itself on
noisy audio (music beds, crowd noise, poor mic).

Nixpkgs ships `whisperx` as a top-level package (3.8.6) - confirmed via
`nix search nixpkgs whisperx`, description "Automatic Speech Recognition
with Word-level Timestamps (& Diarization)" - so this is a real, low-effort
NixOS packaging option, not something to vendor. It pulls in its own
transcription engine (faster-whisper) and a wav2vec2 alignment model per
language (extra download, extra VRAM during the alignment pass).

### 1.4 Recommendation for this axis

Use whisper-server's existing `/inference` with `verbose_json` for the first
increment: zero new services, zero new models, reuses the pinned
`large-v3-turbo` weights, and word timestamps are "good enough" for frame
alignment (frames only need second-level precision, not +-50ms). Track
WhisperX as a **quality upgrade**, not a blocker: if course-builder
transcript quality later demands tighter word boundaries (e.g. highlighting
the exact spoken word under a frame in an interactive course), swap in
`whisperx` behind the same interface, since it is one `nix search` away
from packaged. Do not run WhisperX and whisper-server side by side as two
permanent services; pick one code path per pipeline run.

## 2. Keyframe / scene extraction

### 2.1 ffmpeg scene detection

`ffmpeg -i in.mp4 -vf "select='gt(scene,0.4)',showinfo" -fps_mode vfr
out_%04d.png` extracts frames where the `scene` score (frame-to-frame
content change) exceeds a threshold. Threshold guidance: 0.1-0.3 for subtle
transitions/soft cuts, 0.4-0.5 for standard hard cuts (typical for
presentation/lecture recordings), 0.6-0.7 for only large changes
([FFmpeg scene detection notes](https://gist.github.com/dudewheresmycode/054c8de34762091b43530af248b369e7),
[scene threshold guide](https://salivity.github.io/ffmpeg/article/ffmpeg-scene-change-detection-extracting-frames)).
`showinfo` (or piping through `-f null -` with `scdet` filter) prints each
selected frame's `pts_time`, which is exactly the timestamp needed to align
a frame to the transcript. The dedicated `scdet` filter is built for this:
it can emit rich change-score metadata to stdout for a downstream script to
consume without decoding to images first
([FFmpeg scene detection guide](https://www.ffmpeglab.com/articles/ffmpeg-scene-detection-automated-editing.html)).
`ffmpeg-full` (nixpkgs, 8.1.2) ships these filters; ffmpeg is decode-bound,
so scene-scan cost tracks close to real-time or faster on CPU alone (no GPU
needed for this step; GPU is not the bottleneck for scene score computation).

Pure I-frame extraction (`select='eq(pict_type,PICT_TYPE_I)'`) is a much
cheaper fallback (parses keyframes only, near-instant) but keyframes are an
encoder artifact (GOP boundaries), not content boundaries - not reliable for
"a slide changed" detection on its own.

### 2.2 PySceneDetect

Python/OpenCV scene-cut library with a CLI (`scenedetect`). Two relevant
detectors:

- `detect-content`: frame-to-frame HSV difference vs. a fixed threshold
  (recommended start: 27); good default for hard cuts.
- `detect-adaptive`: two-pass - first computes `detect-content` scores,
  then applies a rolling average over neighboring frames to suppress false
  positives from camera motion/pans; this is the tool's own recommended
  default over plain content detection
  ([PySceneDetect detectors](https://www.scenedetect.com/docs/latest/api/detectors.html)).

CLI usage: `scenedetect --input video.mp4 detect-adaptive list-scenes
save-images` writes a `*-Scenes.csv` with per-scene timecodes and saves
representative frames per scene (by default the first and last frame of each
scene; `-n` requests more, evenly spaced)
([PySceneDetect CLI docs](https://www.scenedetect.com/cli/)). This built-in
"first/last frame of scene" heuristic is a ready-made thumbnail-selection
strategy, arguably better than ffmpeg's raw scene-score frame dump because it
groups frames into scenes first and only samples per scene, avoiding
duplicate near-identical frames around one cut.

Packaged in nixpkgs as `python313Packages.scenedetect` / `python314Packages.
scenedetect` (0.6.7.1) - "Python and OpenCV-based scene cut/transition
detection program & library" (confirmed via `nix search`). No top-level
`pyscenedetect` attribute; consume it as a Python dependency or wrap the
`scenedetect` console script from the Python package set.

### 2.3 ffmpeg vs. PySceneDetect for this use case

ffmpeg's `select=scene` / `scdet` is cheaper (single decode pass, no Python/
OpenCV overhead) and sufficient for "did the picture change." PySceneDetect's
`detect-adaptive` is worth the extra dependency specifically for lecture/
screen-recording video, where camera pans do not apply but slow slide
transitions, cursor movement, and webcam-in-corner overlays produce false
positives under naive frame-diffing; the adaptive rolling-average pass
targets exactly that failure mode. Recommendation: start with ffmpeg
`scdet`/`select=scene` for the first pipeline increment (fewer moving parts,
already have ffmpeg via `ffmpegPath` wired into `pi-voice-stt`'s capture
config), keep PySceneDetect as the documented upgrade path if false-positive
frame counts turn out too high in practice.

### 2.4 Aligning frames to the transcript

Both tools emit frame times in the video's own clock (`pts_time` from
ffmpeg, scene start/end timecodes from PySceneDetect), which is the same
clock the whisper-server transcript's word `start`/`end` fields use (both
measured from t=0 of the same source audio/video). Alignment is a pure
join: for each extracted frame at time T, attach the transcript word(s)
whose `[start, end]` interval contains or immediately precedes T (a small
window, e.g. +-3s, captures "what was being said when this appeared").
No ML step is required for this join - it is arithmetic over two timestamp
lists.

### 2.5 Picking "relevant" frames: heuristics vs. vision-model assist

Three distinct signals a frame extractor conflates today:

1. **Scene change** (camera cut, new shot) - what ffmpeg/PySceneDetect
   directly detect.
2. **Slide/content change** (new bullet, new diagram on an otherwise static
   screen-share) - a subset of scene change with a much lower magnitude
   threshold; screen recordings need a lower `gt(scene, X)` threshold than
   filmed video, or perceptual-hash diffing of a downsampled frame.
3. **Speaker change** - not a visual signal at all; comes from diarization
   (whisper-server's `--diarize`/`tinydiarize` field, or WhisperX's
   diarization) or from a distinct visual cue (new face-cam layout) that a
   generic scene detector will pick up only incidentally.

Local vision-model assisted selection (LLaVA/other VLMs via llama.cpp or
Ollama) adds a **semantic filter after** cheap detection, not a replacement
for it: run ffmpeg/PySceneDetect to get a candidate frame list (already
pruned from "every frame" to "every changed frame," typically 1-5% of raw
frames for a talking-head-plus-slides video), then ask a small VLM per
candidate frame "does this frame contain readable text/a diagram worth
keeping, or is it a transition/blank/face-only frame" to drop noise frames
before persisting them. Running a VLM over _every_ raw frame is the wrong
order of operations - one benchmark found 10 minutes of video at 1 fps
(600 candidate frames) took 20-30 minutes on an RTX 4070 with LLaVA 1.6 7B
at roughly 1s/frame
([local vision models 2026 guide](https://www.promptquorum.com/power-local-llm/local-vision-models-llava-ollama-2026)),
which is acceptable for filtering a few dozen scene-change candidates but
not for scanning a whole video frame-by-frame. MiniCPM-V and LLaVA-Llama3
are cited as good quality/speed tradeoffs for Ollama-hosted local video
frame QA
([Ollama video understanding models](https://milloz.com/info/ai/local-llm-tools/ollama/vision-tools-self-hosting)).

Nixpkgs has both `ollama` (0.32.3, plus an `ollama-vulkan` variant matching
this host's GPU backend) and `llama-cpp`/`llama-cpp-vulkan` (10121)
top-level. Neither ships model weights (GGUF vision models are fetched
separately, same pattern as the Piper voice model in `nix/voice.nix`:
`pkgs.fetchurl` + pinned hash). No LLaVA/MiniCPM-V/Qwen2-VL/BakLLaVA/
Moondream _packages_ exist in nixpkgs (searched individually, no hits) -
expected, since these are model weights, not software; they load through
`ollama pull` or a `llama-cpp` GGUF path, which breaks Nix's normal
build-time hash pinning unless the weight file itself is fetched via
`fetchurl` (as Piper's `.onnx` already is) and pointed at by `ollama`/
`llama-cpp` at run time rather than pulled at run time from the Ollama
registry.

**Recommendation**: ship the heuristic path (ffmpeg/PySceneDetect scene
detection + transcript-time join) as the only step in the first increment.
Treat VLM-assisted frame pruning as an explicit, later, per-item opt-in
(cloud or local) - it is a real quality lever for dense screen-recordings
but adds a second model dependency, GPU contention with whisper-server, and
meaningfully longer per-video processing time, which the vision statement's
"local-first, cloud per modality only when justified" allows deferring
until a concrete case (e.g. very long lecture videos with subtle slide
changes) demonstrates the heuristic path is not good enough.

## 3. Document ingestion (PDF, images)

### 3.1 poppler / pdftotext

`poppler-utils` (nixpkgs, 26.06.0; also `python313Packages.pdftotext` /
`python314Packages.pdftotext` 3.0.0 as a Python binding) is the baseline:
fast, dependency-light, no ML. `pdftotext -bbox file.pdf out.xhtml`
generates per-word bounding boxes in an XHTML/XML structure, and `pdftotext
-layout` preserves reading-order columns/whitespace for plain-text
extraction. This gives page-level and word-level provenance (page number +
bounding box) for **already-digital-text PDFs** with zero OCR cost
([pdftotext bbox](https://manpages.debian.org/jessie/poppler-utils/pdftotext.1.en.html)).
It does nothing for scanned/image-only PDFs.

### 3.2 tesseract (OCR)

nixpkgs ships `tesseract` (5.5.2, current), plus pinned `tesseract3`/
`tesseract4`/`tesseract5` for compatibility. Output formats: plain text,
hOCR (HTML with per-word bounding boxes, confidence, baseline geometry),
TSV (one row per detected word/layout element: page, block, paragraph,
line, word number, left/top/width/height, confidence 0-100, text), ALTO,
PAGE, and searchable/invisible-text PDF
([Tesseract TSV format](https://tomrochette.com/tesseract-tsv-format/),
[Tesseract OCR overview](https://tesseractocr.org/)). TSV or hOCR output is
the right shape for a manifest that needs per-region provenance and a
confidence score to flag low-quality OCR for review.

`ocrmypdf` (nixpkgs, 17.8.1) wraps Tesseract to add a searchable text layer
to an existing PDF (or produce a `--sidecar` plain-text file) while
preserving the original page images - useful when the library wants to keep
the PDF as the canonical artifact and just needs it to become greppable,
rather than converting to Markdown.

### 3.3 marker vs. docling

The user's brief named `marker` (datalab-to/marker, PDF-to-Markdown with
layout awareness). **It is not in nixpkgs** - `nix search nixpkgs marker`
resolves to an unrelated GTK3 Markdown editor
(`legacyPackages.x86_64-linux.marker`, 2023.05.02); no `marker-pdf` package
exists either. Packaging it would mean vendoring a PyTorch + Surya-OCR
dependency stack via poetry2nix/uv2nix - real effort, not a `nix search`
win.

`docling` (IBM/Linux Foundation) **is** in nixpkgs as a top-level package
(2.69.1) plus a family of `python313Packages.docling*` sub-packages
(`docling-core`, `docling-ibm-models`, `docling-parse`, `docling-jobkit`,
`docling-mcp`) and a `docling-serve` HTTP server (1.10.0). Docling's native
output, `DoclingDocument`, is a typed tree that retains **page number,
bounding box, and element type for every chunk**, exportable to Markdown,
HTML, JSON, or DocTags - materially better provenance than Marker's
page-level-only boxes, which lose block-level position once flattened to
Markdown
([Docling vs Marker for RAG](https://docs.bswen.com/blog/2026-04-16-docling-vs-marker-document-parsing/),
[PDF parsing tools comparison 2026](https://builderai.tools/blog/pdf-parsing-for-rag-mineru-docling-marker-compared)).
Docling supports pluggable OCR backends including Tesseract, EasyOCR, and
RapidOCR, so it can subsume tesseract as its OCR engine for scanned
pages rather than needing tesseract wired in separately. Tradeoff: Docling
is heavier and slower per page than Marker (layout model plus optional
TableFormer for tables) - acceptable for a personal library that ingests
occasionally, not for bulk RAG-scale throughput.

### 3.4 Recommendation for this axis

Use `docling` as the primary PDF/image-to-Markdown path: it is packaged in
nixpkgs today, gives page+bbox provenance natively (exactly what citations
in a later course-builder need), and covers both digital-text and scanned
PDFs through its OCR backends. Keep `pdftotext -layout`/`-bbox` as the
cheap fallback for plain digital-text PDFs when Docling's heavier model
pipeline is not needed, and `tesseract`/`ocrmypdf` as standalone tools for
image-only capture (a photographed whiteboard, a screenshot) where there is
no PDF container to begin with.

## 4. End-to-end local pipeline proposal

### 4.1 Stages

```
video/PDF/image URL
      |
      v
[capture]  yt-dlp (video) / direct fetch (PDF, image)
      |
      +-- video --> [audio extract] ffmpeg -vn -> wav 16kHz mono
      |                  |
      |                  v
      |            [transcribe] whisper-server /inference
      |            response_format=verbose_json, token_timestamps=true
      |                  |
      |                  v
      |            transcript.json (segments[].words[]: word,start,end)
      |
      +-- video --> [scene-detect] ffmpeg scdet/select=scene
      |            (or scenedetect detect-adaptive for screen-recordings)
      |                  |
      |                  v
      |            frame_times.csv (pts_time per candidate frame)
      |                  |
      |                  v
      |            [extract] ffmpeg -ss <t> -i video -frames:v 1 frame_NNNN.jpg
      |                  |
      |                  v
      |            [align] join frame_times against transcript words
      |            (+-3s window) -> frames.json (time, path, nearby_words)
      |
      +-- pdf/img --> [convert] docling (Markdown + page/bbox provenance)
      |               fallback: pdftotext -bbox / tesseract tsv+ocrmypdf
      |
      v
[persist]  library CLI: hash blobs, write manifest, place under
           ignored library dir; manifest tracked in Git (the-den)
```

### 4.2 Rough compute cost, single desktop GPU (Vulkan)

Baseline host: NVIDIA GPU already running `whisper-cpp-vulkan` for
interactive STT (proprietary driver, Vulkan backend, no CUDA toolkit
wired in - `hardware.nvidia` in `nix.dotfiles/hosts/nixos/default.nix`).

- **Transcription**: `large-v3-turbo` on a discrete NVIDIA GPU via Vulkan
  runs well above real-time - published large-v3-turbo real-time-factor
  figures on GPU are in the tens-to-~100x range, and Vulkan trails CUDA by
  roughly 10-30% on the same silicon
  ([Whisper large-v3-turbo benchmark notes](https://whispernotes.app/blog/introducing-whisper-large-v3-turbo)).
  A 1-hour video should transcribe in well under 5 minutes of wall time,
  GPU-bound only briefly; CPU/decode overhead for chunking dominates for
  short items.
- **Scene detection**: ffmpeg `scdet`/`select=scene` is a single decode
  pass, CPU-bound, no GPU use - close to real-time (roughly 1x video
  duration) on one CPU core, faster with ffmpeg's built-in threading.
  PySceneDetect's `detect-adaptive` is somewhat slower due to Python/OpenCV
  overhead but still sub-real-time-multiple for typical lecture-length
  video.
- **Frame extraction**: near-free; `-frames:v 1` seeks are I/O-bound,
  milliseconds each, tens of frames per video.
- **VLM-assisted frame filtering (optional, deferred)**: roughly 1s/frame
  on a mid-range discrete GPU with a 7B-class vision model (LLaVA 1.6 7B
  reference point); run only against the handful of scene-change
  candidates (tens, not thousands), so total added cost is low single-digit
  minutes per video, not the 20-30 minutes quoted for exhaustive 1fps
  scanning.
- **Docling PDF conversion**: page-by-page layout model inference; for a
  personal library's occasional-ingest workload (not bulk RAG corpus
  building) this is acceptable even without GPU acceleration - budget
  low seconds per page on CPU, faster with the layout model on GPU.
- **Tesseract/ocrmypdf**: CPU-only, roughly 1-2s per page for scanned
  text, no GPU needed.

Net: for the flagship video case, the GPU is only meaningfully exercised
during transcription (minutes, already true today for interactive STT) and
optionally during VLM frame filtering (also minutes, deferred). Scene
detection, frame extraction, and document conversion are CPU-bound and cheap
enough to run inline in a capture flow without a dedicated worker queue,
though a job queue (Scufris already has one - `scufris_job_*` native tools
per `NOTES.md`) is still the right home for anything video-length, since
whisper-server should not be tied up mid-conversation by a batch job.

### 4.3 NixOS packaging summary

| Tool                             | nixpkgs status             | Attribute                                                    | Notes                                                                                          |
| -------------------------------- | -------------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------------------------- |
| whisper.cpp / whisper-server     | packaged, already deployed | `whisper-cpp-vulkan`                                         | reuse existing service                                                                         |
| ffmpeg (scene filters)           | packaged                   | `ffmpeg-full` (or plain `ffmpeg`)                            | already used by `pi-voice-stt` for capture                                                     |
| PySceneDetect                    | packaged                   | `python3XXPackages.scenedetect`                              | no top-level `pyscenedetect` attr                                                              |
| faster-whisper                   | packaged                   | `python3XXPackages.faster-whisper`, `wyoming-faster-whisper` | optional, skip for v1                                                                          |
| WhisperX                         | packaged                   | `whisperx` (top-level)                                       | quality upgrade path, not v1                                                                   |
| yt-dlp                           | packaged                   | `yt-dlp` (top-level)                                         | video capture front door                                                                       |
| poppler-utils / pdftotext        | packaged                   | `poppler-utils`, `python3XXPackages.pdftotext`               | digital-text PDFs                                                                              |
| tesseract                        | packaged                   | `tesseract` (5.5.2)                                          | OCR engine                                                                                     |
| ocrmypdf                         | packaged                   | `ocrmypdf` (17.8.1)                                          | searchable-PDF wrapper around tesseract                                                        |
| docling                          | packaged                   | `docling` (top-level, 2.69.1)                                | primary PDF/image-to-Markdown path                                                             |
| marker (datalab-to/marker)       | **not packaged**           | n/a                                                          | `marker` attr is an unrelated GTK app; would need poetry2nix/uv2nix vendoring                  |
| ollama                           | packaged                   | `ollama`, `ollama-vulkan`                                    | for local VLM frame filtering, deferred                                                        |
| llama-cpp                        | packaged                   | `llama-cpp`, `llama-cpp-vulkan`                              | alternative VLM host                                                                           |
| LLaVA/MiniCPM-V/Qwen2-VL weights | not applicable             | n/a                                                          | model weights, fetch via `pkgs.fetchurl` like Piper's `.onnx`, same pattern as `nix/voice.nix` |
| mpv                              | packaged                   | `mpv`                                                        | for any human "watch the source" fallback, not part of the pipeline itself                     |

All `nix search nixpkgs <name>` lookups above were run live against the
current nixpkgs channel on this host (2026-08-24) and are reproducible with
the same command.

## Recommended pipeline

1. **Transcription**: reuse the already-running `whisper-server`
   (`whisper-cpp-vulkan`, `large-v3-turbo`) with a batch-mode call to
   `/inference` using `response_format=verbose_json` and
   `token_timestamps=true`/`split_on_word=true`. No new service, no new
   model. Persist the `segments[].words[]` array as the word:timestamp
   transcript. Revisit `whisperx` (already packaged, `whisperx` top-level
   attribute) only if course-builder quality later demands wav2vec2-grade
   +-50ms alignment over whisper.cpp's DTW-based +-500ms-class timestamps.
2. **Frames**: `ffmpeg` `scdet`/`select='gt(scene,0.4)'` (threshold tuned
   down for screen-recording-style sources) to get candidate frame
   timestamps, extract with `-frames:v 1` seeks, join against transcript
   word intervals by time (+-3s window) for provenance. Treat
   PySceneDetect's `detect-adaptive` as the documented upgrade if ffmpeg's
   naive diffing produces too many false positives on slide-heavy content.
   Defer local-VLM (Ollama/llama.cpp + a fetched GGUF vision model, same
   `fetchurl` pattern as Piper) frame pruning to a later, explicitly
   opt-in stage - it is a real lever but a second model dependency and GPU
   contention risk, not needed for a first working pipeline.
3. **Documents**: `docling` as the primary PDF/image-to-Markdown converter
   for its native page+bbox provenance (packaged in nixpkgs today);
   `pdftotext -bbox`/`-layout` as a cheap fallback for plain digital-text
   PDFs; `tesseract`/`ocrmypdf` for pure-image capture with no PDF
   container. Do not attempt to package `marker` - it is unpackaged and
   Docling already covers the provenance requirement better.
4. **Persistence**: every stage above writes plain files (transcript JSON,
   extracted JPEG/PNG frames, Markdown + provenance JSON for documents)
   into the library's ignored blob directory; the library CLI's narrow
   write surface (already decided in `NOTES.md`) hashes these and writes
   the tracked manifest - this research does not change that division of
   labor, it only defines what the CLI receives as input blobs.
5. **Scheduling**: route ingestion runs through Scufris's existing job
   queue (`scufris_job_*`) rather than inline in a conversation turn, since
   video-length transcription should not block `whisper-server`'s
   interactive STT path used by the voice HUD.
6. **Cloud exception**: none justified for v1. Every step above has a
   packaged, local, Vulkan-or-CPU-bound implementation with acceptable cost
   for a personal library's occasional-ingest workload. If a future case
   needs OCR/VLM quality beyond Tesseract/Docling/local-VLM (e.g. dense
   handwriting, non-Latin scripts Tesseract handles poorly), that is the
   concrete trigger to evaluate one cloud call per item with the tradeoff
   stated at that time, not a default now.
