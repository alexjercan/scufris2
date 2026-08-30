# Add remote iOS voice transcription

- STATUS: CLOSED
- PRIORITY: 90
- TAGS: ios, audio, api

## Goal

Add hold-to-dictate to the iOS pill. Record on the phone, transcribe through the
host's private `ai-tools-api`, review on the phone, then submit ordinary text.

## Decisions

- The iPhone owns microphone permission, recording, duration, cancellation, and
  deletion of local temporary audio.
- Inference stays on the host. Do not use Apple Speech or expose
  `ai-tools-api` directly to the tailnet.
- Turn the existing authenticated surface gateway on port 10440 into the remote
  HTTP and WebSocket API; do not add another public listener.
- Keep protocol-v4 WSS on `GET /` and `GET /surface`. Add
  `POST /audio/transcription` on the same bearer-token boundary. Deployment
  prefixes such as `/scufris-staging` belong only to the reverse proxy and must
  never appear in the gateway route table.
- The gateway validates a bounded mono PCM WAV and forwards it directly to the
  loopback `ai-tools-api` route `/v1/audio/transcriptions`. A service-owned
  content socket remains future attachment work; transcription has no durable
  content to store.
- The response is bounded JSON `{ "text": "..." }`. Never log audio or
  transcript text.
- The transcript enters the mock's editable review state. Sending it uses the
  unchanged protocol-v4 text request.
- Dictation comes before speech playback, mute, or background audio.

## Acceptance

- Hold starts one local recording with an explicit iOS privacy boundary;
  release starts transcription; cancel deletes the take.
- Upload size, duration, content type, response size, and deadline are bounded.
- Authentication failures and API failures are clear and do not submit text.
- The transcript is editable and requires explicit send or discard.
- Protocol v4 remains byte-for-byte unchanged.
- Gateway and Swift simulator tests pass. Physical-device review is part of the
  combined 1.1.0 release candidate.

## Verification

- Seven focused Rust gateway tests pass. They prove strict protocol-v4 WSS
  bridging, bearer authentication, loopback-only inference, raw upload media
  rejection, strict WAV format and duration bounds, multipart forwarding, and
  bounded transcript JSON.
- The service package and focused Home Manager interface and activation checks
  build with the new API dependencies and shared `aiToolsApi.baseUrl`.
- All 68 Node tests and 93 Python helper tests pass. Gateway Clippy passes with
  warnings denied.
- iOS workflow run
  [33319815589](https://github.com/alexjercan/scufris2/actions/runs/33319815589)
  builds with Xcode 26.3 and passes simulator tests for endpoint derivation and
  dictation presentation state.
- Documentation workflow run
  [33319815579](https://github.com/alexjercan/scufris2/actions/runs/33319815579)
  passes.
- No Home Manager activation or TestFlight upload was performed. Those remain
  coordinated release-candidate work.
