Use `/pair`. Create and implement a new standalone repository at
`/home/alex/personal/ai-tools-api`. Work directly on its `master` branch. Do not
create a GitHub repository, remote, pull request, or worktree. Do not modify
Scufris or nix.dotfiles during this implementation.

The product is a private API for small reusable AI-backed tools. Its first
release is strictly speech-only. Do not name Scufris as a consumer in its code,
package metadata, README, examples, tests, documentation, configuration, or
module options. Other projects consume it as an ordinary HTTP API.

Choose Python for the API. This service is mostly bounded HTTP validation,
subprocess supervision, and adaptation around existing inference executables.
Use FastAPI and Uvicorn because correct multipart parsing and ASGI lifecycle are
a concrete need. Keep the dependency set narrow. Use Python 3 type hints, Ruff,
mypy, and pytest. The broader repository name does not authorize LLM
completions, embeddings, generic providers, or platform abstractions without a
separate tracked request.

## Source material to inspect first

Read these existing deployments completely before scaffolding so the new flake
owns their pinned behavior rather than inventing different models:

- `/home/alex/personal/nix.dotfiles/home/modules/agents/pi/extensions/voice-stt/module.nix`
- `/home/alex/personal/nix.dotfiles/home/modules/agents/checks.nix` around the
  local Whisper checks
- `/home/alex/personal/scufris2/nix/voice.nix`
- `/home/alex/personal/scufris2/nix/piper-stdout.patch`
- `/home/alex/personal/scufris2/tools/voice/scufris-speak`
- `/home/alex/personal/scufris2/nix/checks/voice.nix`

Carry forward these current defaults unless a focused build proves they are no
longer valid:

- Whisper package: `pkgs.whisper-cpp-vulkan` for the deployed Linux host.
- Whisper executable: `whisper-server`.
- Whisper model:
  `ggml-large-v3-turbo-q5_0.bin` from
  `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin`
  with hash `sha256-OUIhcJzVrR9AxG5gMcphvOiJMebgiMGIKUxtWlX/p+I=`.
- Whisper language default: `auto`.
- Piper package: `pkgs.piper-tts` with training, HTTP, and alignment disabled.
- Preserve the required Piper stdout-close patch from the source material if
  nixpkgs still needs it. Prove whether it is needed; do not apply a stale patch
  blindly.
- Piper voice: `en_US-lessac-medium`.
- Voice model URL:
  `https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx`
  with hash `sha256-Xv4J5pkCGHgnr2RuGm6dJp3udp+Yd9F7FrG0buqvAZ8=`.
- Voice config URL:
  `https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json`
  with hash `sha256-7+GcQXvtBV8taZCCSMa6ZQ+hNbyGiw5quz2hgdq2kKA=`.

The new repository must own these packages, patches, model fetches, service
configuration, and tests. A later separate change will remove the old ownership
from nix.dotfiles and other repositories only after this API is proven.

## Step 1: repository scaffold

Create the directory and initialize Git directly on master:

```text
mkdir -p /home/alex/personal/ai-tools-api
cd /home/alex/personal/ai-tools-api
git init -b master
```

Create tested, non-placeholder project infrastructure before API behavior:

- `AGENTS.md` with repository architecture, workflow, Python, Nix, process,
  bounds, test, and documentation rules.
- `CLAUDE.md` containing only `@AGENTS.md`.
- `.scufris.toml` with conventions for tracking, master-branch work, Python
  checks, Nix checks, and review. Keep it generic to this repository.
- `.agents/skills/pair/SKILL.md`, adapted from the current Pair skill.
- `.agents/skills/tatr/SKILL.md`, adapted for Markdown tasks in this repository.
- `.agents/skills/api-service/SKILL.md` for API, backend, bounds, and focused
  tests.
- `.agents/skills/nix-package/SKILL.md` for flake packages, models, modules, and
  Nix checks.
- `tasks/<id>/TASK.md` for this tracked implementation, created with Tatr after
  the scaffold supports it. Record decisions and verification evidence there.
- `README.md` with only a short description and Quickstart.
- Durable documentation under `docs/`, including API contract, configuration,
  development, deployment, and security boundaries.
- `.gitignore`, `pyproject.toml`, package source directory, tests directory, and
  GitHub workflow directory.

Use these names consistently:

```text
repository and distribution: ai-tools-api
Python import package:       ai_tools_api
executable:                  ai-tools-api
flake package and app:       ai-tools-api
Home Manager namespace:      services.ai-tools-api
```

Do not copy Scufris-specific extension, desktop, widget, or orchestration rules.
Do not add empty skills or placeholders. Each skill must contain its first real
workflow and checks.

## Step 2: exact API scope

Expose exactly two public inference endpoints. Disable FastAPI's automatic
`/docs`, `/redoc`, and `/openapi.json` routes. Unknown routes return 404. Do not
add LLM completion, embedding, model-management, account, billing, or generic
provider endpoints.

### Speech to text

```http
POST /v1/audio/transcriptions
Content-Type: multipart/form-data
```

Supported fields:

- `file`: required bounded non-empty audio upload.
- `model`: required; initially only `whisper-1`.
- `language`: optional; default `auto`.
- `response_format`: optional; initially only `json`.

Successful response:

```json
{"text":"The transcribed message."}
```

The API validates and bounds the upload before handing it to the managed
loopback `whisper-server`. Normalize whisper.cpp output into the exact response
above. Do not expose the whisper.cpp `/inference` endpoint publicly.

### Text to speech

```http
POST /v1/audio/speech
Content-Type: application/json
```

Initial request:

```json
{
  "model":"piper-1",
  "voice":"en_US-lessac-medium",
  "input":"The build passed.",
  "response_format":"wav"
}
```

Supported values are initially exact:

- model: `piper-1`.
- voice: `en_US-lessac-medium`.
- response format: `wav`.

Return validated WAV bytes as `audio/wav`. Invoke the owned Piper executable
without a shell, with the pinned model and config. Capture bounded stdout,
stderr, exit status, timeout, and cancellation. Validate the RIFF/WAVE structure
before returning it. The API synthesizes audio only; it never plays it.

### Errors

Use one small OpenAI-shaped error body for both endpoints:

```json
{
  "error":{
    "message":"Short human-readable explanation.",
    "type":"invalid_request_error",
    "code":"audio_too_large"
  }
}
```

Define stable bounded codes for invalid content type, unsupported model, voice,
format, empty input, oversized input, invalid audio, backend unavailable,
backend failure, timeout, and overloaded concurrency. Do not leak paths,
commands, stderr, model internals, or tracebacks to clients.

Set explicit conservative limits for:

- request line and headers where the server permits configuration;
- multipart upload bytes;
- text UTF-8 bytes;
- subprocess output bytes;
- concurrent STT requests;
- concurrent TTS requests;
- backend startup and request duration; and
- error detail bytes.

Document every initial value. Reject work before allocating or spawning when a
bound can be checked first.

## Step 3: owned inference runtime

The default `nix run .` must provide the complete local service. It must not
require separately installed Whisper, Piper, models, Python packages, or shell
setup.

Implement one foreground supervisor process that:

1. Starts its owned `whisper-server` child on loopback only.
2. Passes the pinned Whisper model, `--language auto`, and a private inference
   path.
3. Waits for bounded readiness without broad process matching.
4. Starts the ASGI API on configurable host and port, defaulting to
   `127.0.0.1` and a documented port.
5. Invokes owned Piper processes on demand.
6. On SIGINT, SIGTERM, startup failure, or API shutdown, terminates only child
   PIDs it recorded, waits a bounded grace period, then kills only those owned
   PIDs if necessary.
7. Preserves meaningful exit codes and never invokes a shell.

Use command arrays, trusted absolute executable/model paths supplied by the Nix
wrapper, private temporary/runtime directories, bounded files, and deterministic
cleanup. Never use `pkill`, `killall`, broad process matching, or an unbounded
temporary upload.

The Python package must also support dependency injection of fake Whisper and
Piper backends so ordinary tests do not download models or require Vulkan.

## Step 4: flake.nix and deployment

Create and lock `flake.nix` as a first-class interface. Use nixpkgs and
flake-parts unless a simpler plain flake is demonstrably clearer. Support at
least `x86_64-linux`; add `aarch64-linux` when all selected packages evaluate.
Do not claim unsupported Darwin behavior.

Required outputs:

- `packages.<system>.default`: complete wrapped API runtime.
- `packages.<system>.ai-tools-api`: same explicit package.
- `apps.<system>.default`: run the complete local API.
- `apps.<system>.ai-tools-api`: explicit app alias.
- `devShells.<system>.default`: Python, Ruff, mypy, pytest, curl, jq, and useful
  Nix development tools.
- `formatter.<system>`.
- `checks.<system>` for formatting, lint, typing, unit tests, protocol smoke,
  package closure, and module evaluation.
- `homeModules.default`: a disabled-by-default Home Manager module that runs the
  complete service as a hardened systemd user unit.

The Home Manager module should use `services.ai-tools-api` and expose bounded
options for:

- enable;
- package;
- public API host and port;
- Whisper package and pinned model;
- internal Whisper loopback port;
- Piper package, voice model, and voice config;
- STT/TTS concurrency and timeout limits; and
- optional environment-file path only if a concrete secret is introduced.

Defaults bind the API to loopback. Document how a deployment may bind a private
Tailscale address, but add no public exposure, reverse proxy, Funnel, router
forwarding, account system, or application token without a separate decision.
The internal whisper.cpp endpoint always remains loopback-only.

The systemd unit must have focused hardening appropriate for model execution and
temporary audio: `NoNewPrivileges`, private temporary storage, strict system
protection with necessary writable runtime paths, bounded restart behavior, and
clean child shutdown. Do not invent hardening that prevents Vulkan/model access;
verify the actual unit.

`nix run .` must be locally testable with curl fixtures for both endpoints.
Document exact commands that create or locate a small audio fixture, transcribe
it, synthesize WAV, inspect headers, and stop the foreground service.

## Step 5: tests

Use cheap deterministic tests first. Unit and integration tests must not require
real model downloads, a GPU, microphone, speaker, network, or GitHub.

Required coverage:

- exact route and method acceptance;
- disabled docs/OpenAPI routes;
- multipart parsing and bounded upload rejection;
- required and unsupported STT fields;
- required and unsupported TTS fields;
- strict UTF-8 and control-character policy for speech input;
- normalized transcription response;
- OpenAI-shaped bounded errors;
- timeout, cancellation, backend failure, and overload behavior;
- no shell invocation;
- child PID ownership and bounded shutdown;
- valid and malformed WAV handling;
- no stderr/path leakage;
- separate STT and TTS concurrency bounds;
- fake backend end-to-end HTTP requests;
- `nix run`-equivalent smoke with fake packaged backends;
- flake output evaluation and package closure composition;
- Home Manager module disabled and enabled evaluation; and
- exact pinned model URLs and hashes.

Add one opt-in real inference check for local use that exercises the pinned
Whisper and Piper assets. Do not put the large real-model check in ordinary
GitHub CI unless measured runtime and cache behavior justify it.

## Step 6: GitHub Actions without a remote

Create workflows now, but do not create or push a GitHub repository.

`check.yml` must run on pushes to master, pull requests, and manual dispatch. It
should use official checkout, Python, and Nix setup actions; install from locked
metadata; run the Python focused checks; and run `nix flake check -L`. Add action
version pins and least-privilege read permissions.

Add a tag-triggered `release.yml` as real delivery: rerun checks, build the
Python wheel and source distribution, build the default Nix package, and publish
the wheel, source archive, checksums, and release notes to a GitHub Release. Use
read-only permissions in ordinary jobs and grant `contents: write` only to the
release job. The workflow remains dormant until a repository and version tag
exist. Do not add a fake host deploy job, invented secret names, or a destination
that does not exist. Record host deployment as deferred until a concrete target
is configured. Validate workflow syntax locally with a packaged checker when
practical.

## Quality and documentation rules

- Keep handlers thin. Put validation, Whisper adaptation, Piper execution, WAV
  validation, process ownership, and configuration in small typed modules.
- Keep model-facing and client-facing errors separate from operational logs.
- Log request IDs, endpoint, duration, result code, and backend status without
  logging uploaded audio or synthesis text by default.
- Use temporary files only where whisper.cpp requires them. Create them in a
  private bounded directory and remove them deterministically.
- Never add telemetry, analytics, or external calls beyond pinned model fetches.
- Keep README to description and Quickstart. Put durable API, operations,
  architecture, and deployment material under `docs/`.
- Add files with their first tested behavior. Do not add empty placeholders.
- Preserve user authorship. Add no AI attribution or co-author trailers.

## Verification and completion

Run the cheapest relevant check after each batch. Before completion run, at
minimum:

```text
ruff format --check .
ruff check .
mypy src
pytest
nix flake check -L
nix build .#ai-tools-api
```

Then run the packaged API locally with fake backends and curl both endpoints.
Run real Piper locally. Run real Whisper only when the model/Vulkan cost is
acceptable, and record why if deferred. Record all commands and outcomes in the
repository task.

Completion requires:

- a clean standalone Git repository on master with no remote;
- complete scaffold and non-placeholder skills;
- exactly the two public endpoints;
- deterministic fake-backend HTTP proof;
- complete `nix run .` runtime composition;
- pinned owned Whisper and Piper packages/models;
- a tested Home Manager module ready to replace external ownership later;
- GitHub check workflow ready for a future repository;
- no mention of Scufris as a consumer in the new repository; and
- no edits to existing consumer or nix.dotfiles repositories.

Continue through approved mechanical work without asking whether to proceed.
Stop only when a decision can materially change the API contract, package
closure, deployment security, or tested outcome. Report `Delta`, `Verified`, and
`Next` at every such stop.
