# Showing a reference: "look what I found"

Research question: how can Scufris visually surface a reference (a web page, a
saved article snapshot, a library item, a den note) to the user, given the
current dashboardd architecture. Grounded in the local dashboardd source
(`~/personal/dashboardd`) as of this session, plus web research for prior art.

Hard constraint restated: dashboardd must not gain Scufris-specific hooks.
Only generic, Scufris-agnostic widgets or dashboardd features are in scope for
changes to dashboardd itself.

## 1. Grounding: what dashboardd actually does today

Files read: `crates/dashboardd-desktop/src/service.rs`,
`crates/dashboardd-desktop/src/launcher.rs`,
`crates/dashboardd-desktop/src/main.rs`,
`crates/dashboardd-desktop/tauri.conf.json`,
`crates/dashboardd-desktop/frontend/src/{surface.ts,surface.html}`,
`crates/dashboardd-runtime/src/{instance.rs,widget.rs}`,
`crates/dashboardd-desktop-control/src/lib.rs`,
`docs/src/widget-authoring/{index,frontend,backend-protocol,packaging}.md`,
`widgets/tatr-tasks/{widget.toml,src/main.rs,frontend/src/details.ts}`,
and (on the Scufris side) `skills/dashboard/SKILL.md`,
`extensions/scufris/dashboard/index.ts`, `tools/dashboard/scufris-dashboard`.

Key facts:

- Tauri version is `2.11.5` (`crates/dashboardd-desktop/Cargo.toml:17`,
  confirmed in `Cargo.lock`). No `tauri-plugin-opener`, `tauri-plugin-shell`,
  or any other Tauri plugin is a dependency anywhere in the workspace
  (checked `Cargo.lock` for `tauri-plugin`).
- Every window dashboardd-desktop creates uses
  `WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html"))`
  (widget surfaces, `service.rs:857-867`) or
  `WebviewUrl::App("launcher.html")` (launch dialogs, `launcher.rs:168`).
  `WebviewUrl::External` is never used anywhere in the crate
  (`grep -rn "WebviewUrl" crates` returns only these two `App(...)` calls).
  There is currently no code path that opens a Tauri window against an
  arbitrary URL.
- The app-wide CSP in `tauri.conf.json:13` is:
  `default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; script-src 'self' dashboardd-widget:; connect-src ipc: http://ipc.localhost; object-src 'none'; frame-src 'none'`.
  This is a single global string (`app.security.csp`), applied to all windows
  that load `WebviewUrl::App(...)` pages. `frame-src 'none'` and
  `object-src 'none'` mean **no `<iframe>`, `<object>`, or `<embed>` can be
  used anywhere in dashboardd-desktop today**, including srcdoc/blob-URL
  iframes and PDF `<object>` embeds. `img-src` only allows same-origin and
  `data:` URIs, so any inlined snapshot content must ship images as `data:`
  URIs, not remote `https://` URLs.
- Widget frontends are ES modules loaded via `import(snapshot.frontend_url)`
  where `frontend_url` is `dashboardd-widget://localhost/<surface_id>.js`
  (`surface.ts:69-71`, served by `widget_protocol` in `service.rs:379-430`).
  They mount into `<section id="widget">` inside `surface.html`
  (`surface.html:10`) - **the same document/JS realm as the trusted shell
  chrome** (health banner, restart button, IPC bridge). Each widget attaches
  its own Shadow DOM root, but per
  `docs/src/widget-authoring/index.md:29`: _"Frontend code runs in the
  dashboard page inside a Shadow DOM mount. Shadow DOM isolates styles, not
  authority."_ This is dashboardd's own explicit statement that Shadow DOM
  is not a security boundary. The trust boundary doc
  (`docs/src/widget-authoring/index.md:25-29`) states dashboardd treats an
  _installed widget package_ (backend binary + frontend JS) as trusted local
  software - the whole model assumes the widget author is trusted, not that
  arbitrary data rendered by a widget is safe to execute.
- Widget instances are backed by a real OS process
  (`crates/dashboardd-runtime/src/instance.rs`, `run_backend`, spawns
  `config.backend` and speaks JSON Lines over stdin/stdout, see
  `docs/src/widget-authoring/backend-protocol.md`). The frontend only ever
  receives JSON via `update(payload)` (from backend `update` messages) or
  direct typed inputs (`{"type": "<manifest-type>", "value": <json>}`,
  `dashboardd-runtime/src/instance.rs:38-43`, validated by exact type string
  in `normalize_inputs`, `instance.rs:410-427`). The frontend contract doc
  explicitly warns: _"Options and frontend state are visible browser data.
  Do not place secrets or sensitive paths in either"_ and _"Values and
  dynamic output payloads are public browser data. Do not include secrets or
  filesystem paths"_ (`docs/src/widget-authoring/frontend.md:62,82`).
- **Existing prior art for rendering captured/untrusted-ish content already
  lives in this codebase.** `widgets/tatr-tasks` has a `details` variant
  (`widget.toml:16-23`) with a typed input port
  `tatr.task-artifact-reference/v1` (a `{project_id, worktree_id, task_id,
artifact}` reference, not a raw path) and a backend
  (`widgets/tatr-tasks/src/main.rs`) that resolves the reference to a file
  under a canonicalized, `starts_with`-checked task directory
  (`resolve_task_directory`, `main.rs:874-891`), classifies it into an
  `ArtifactKind` of `markdown | html | text | image`
  (`classify_artifact`, `main.rs:962-987`, size-capped at 256 KiB text /
  2 MiB image via `MAX_TEXT_ARTIFACT_BYTES` / `MAX_IMAGE_ARTIFACT_BYTES`),
  and ships the bytes to the frontend as UTF-8 text (or base64 for images)
  inside an `update` payload. The frontend
  (`widgets/tatr-tasks/frontend/src/details.ts`) renders:

  - `markdown` via `marked.parse()` then `DOMPurify.sanitize()` with
    `FORBID_TAGS: ["img","style","iframe","object","embed"]`
    (`details.ts:228-248`);
  - `html` via `DOMPurify.sanitize()` directly, forbidding `script, style,
form, input, button, select, textarea, iframe, object, embed, img,
audio, video, source, link, meta, base` and the `class/id/style`
    attributes (`details.ts:250-279`);
  - `image` via a `data:<mime>;base64,...` `<img src>` (`details.ts:194-201`,
    consistent with `img-src 'self' data:` in the CSP);
  - `text` via `textContent` into a `<pre>` (`details.ts:186-193`, never
    parsed as markup).

  This is exactly "sanitize untrusted markup at the DOM level" rather than
  "isolate untrusted content in its own browsing context" - the latter is
  structurally unavailable because of `frame-src 'none'` / `object-src
'none'`. `secureLinks()` (`details.ts:281-`) also strips `href` from any
  link that is not `https?://` or a known sibling artifact, and sets
  `target="_blank" rel="noopener noreferrer"` on external links - but since
  no `tauri-plugin-opener`/shell plugin is wired up anywhere in the
  workspace, this `target="_blank"` currently has no confirmed effect; Tauri
  2's WRY backend on Linux does not open a new OS window for it without an
  explicit `on_new_window_requested`/opener handler, so external links are
  effectively inert today (unverified without a live click-through test -
  flagged as a gap either way, not a working "open in real browser" hook).

- The desktop control protocol (`dashboardd-desktop-control/src/lib.rs`) is
  a closed `Command` enum: `Discover | Open{widget_id, variant_id, options,
inputs, presentation} | Update | List | Focus | Close | Quit`. `Open`
  always resolves a widget id + variant id through
  `WidgetsManager`/`InstanceManager` - there is no "open this arbitrary URL"
  command today, by design.
- On the Scufris side, `skills/dashboard/SKILL.md` and
  `extensions/scufris/dashboard/index.ts` show Scufris already treats
  dashboardd purely through this typed, widget-shaped contract
  (`scufris_widget_open` with `widget_id/variant_id/options/inputs/
presentation`), backed by `tools/dashboard/scufris-dashboard`, a private
  Python helper that speaks the control-socket JSON protocol directly. This
  is the existing integration seam Scufris already uses for every dashboard
  interaction.

## 2. Options

### (a) Generic "viewer" widget (sanitized render, in-process)

A new widget package (e.g. `dashboardd-reference-viewer`, or extend
tatr-tasks-style pattern generically) with a typed input port such as
`dashboardd.reference/v1` = `{ "path": "<opaque-token-or-id>" }` **not** a
raw filesystem path in the JSON value (per the frontend contract's "do not
include filesystem paths" guidance) - the backend process resolves the
opaque id to a real path itself (it already has filesystem access, same
pattern as `resolve_task_directory`).

- **Frontend rendering strategy.** iframe/srcdoc, `<object>`, and blob-URL
  iframes are **not viable** under the current CSP (`frame-src 'none'`,
  `object-src 'none'`) unless dashboardd's CSP is loosened globally (see
  caveat below). The only viable strategy that matches the existing engine
  is the tatr-tasks pattern: backend reads/converts the captured content
  (single-file HTML snapshot, article text, image) into one of a small set
  of typed kinds, ships it as JSON over the backend protocol, and the
  frontend renders it via `DOMPurify.sanitize()` into a Shadow DOM subtree -
  never via `innerHTML` of raw content, never via navigation. PDFs have no
  safe rendering path this way (no `<object>`, no PDF.js iframe); a PDF
  reference would need pre-rasterization to `image` artifacts (one Article
  page → one PNG) or be excluded from this widget and handled by (c)/(d).
- **Security.** This is real sanitization-based isolation, not
  context-isolation. It's proportionate to _simplified/text-like_ captures
  (readability-extracted article text, markdown notes, screenshots) but
  actively lossy and imperfect for full webpage snapshots: DOMPurify with
  the tatr-tasks-style forbid-list removes `<script>`, all form/media/embed
  elements, and `class/id/style` attributes, which also strips most CSS
  layout and any interactive behavior from a real page capture - so
  fidelity to "what the page actually looked like" is low. A CSS-preserving
  variant would need a much larger, security-reviewed allowlist (inline
  `style` attributes are a known DOMPurify/CSS-injection risk surface -
  `data:` URI CSS `url()` exfiltration, `expression()`-class legacy attacks,
  etc.) - meaningfully more implementation and audit cost than the
  tatr-tasks precedent, which deliberately forbids `style`.
- **Widget/window lifecycle vs. Scufris control.** Fits the existing
  `Open{widget_id, variant_id, inputs, presentation}` /
  `Update`/`Focus`/`Close` control protocol exactly, with zero dashboardd
  core changes - this is the only option of the four that requires _no_
  changes to `dashboardd-desktop` or `dashboardd-runtime` at all, only a new
  widget package dropped on `DASHBOARDD_WIDGET_PATH`. Scufris already has
  this integration path end-to-end (`skills/dashboard/SKILL.md`,
  `extensions/scufris/dashboard/index.ts`,
  `tools/dashboard/scufris-dashboard`).
- **Citation → display mapping.** Natural: a "reference" manifest (file +
  metadata) maps 1:1 to a typed input value the launch frontend or direct
  `Open` call supplies; the backend is the only thing that touches the real
  path, matching the "task artifact reference" precedent exactly.
- **Cost.** Low-medium. Mirrors an already-shipped, already-tested pattern
  (widget.toml + Rust/any-language backend + TS frontend with
  `marked`+`DOMPurify`). Main work is: (1) a content-normalization step that
  turns "whatever was captured" (readability HTML, raw HTML snapshot, PDF,
  image) into one of the supported `ArtifactKind`-like buckets before it
  ever reaches dashboardd, and (2) deciding how much CSS/layout fidelity to
  sacrifice for sanitization safety.

### (b) Generic "url" widget or small dashboardd feature (real WebviewWindow at an external URL)

Point a Tauri `WebviewWindow` directly at `https://...` (live page) or
`file://...` (local snapshot HTML) via `WebviewUrl::External(url)`.

- **Feasibility given current code.** Requires real dashboardd-desktop
  changes, not just a new widget: `create_surface_for_instance` in
  `service.rs:812-907` hard-codes `WebviewUrl::App("index.html")` and ties
  every surface to a widget/variant/instance; there is no window-creation
  path independent of the widget model. A generic (Scufris-agnostic)
  feature would need: a new `Command::OpenExternal{ url, title, presentation
}`-shaped variant in `dashboardd-desktop-control::Command` (protocol
  version bump), a parallel branch in `execute_command`/window creation
  that skips `InstanceManager` entirely and builds a bare
  `WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))`, plus
  `dashboardctl` CLI support. This is bounded but is a real, generic
  addition to dashboardd's surface concept - "open an external URL in a
  native window" is not widget-specific and would not encode anything about
  Scufris, so it satisfies the "no Scufris-specific hooks" constraint, but
  it does add a second, non-widget kind of surface to the core model.
- **Isolation.** Strong instance isolation is inherent: it's a separate
  `WebviewWindow` (separate WebKitGTK web view) from the trusted `index.html`
  shell - no shared DOM, no shared Shadow-DOM host, full script execution
  and CSS fidelity available (real browser rendering of a real snapshot or
  live page). This is qualitatively better isolation than (a)'s
  same-document sanitize-and-inject approach for anything that needs to
  look like the original page.
- **CSP caveat (found in prior art, not just inferred).** Tauri 2 has open,
  documented problems applying/scoping CSP and IPC correctly when a window
  uses `WebviewUrl::External`: see
  [tauri-apps/tauri#8476, "CSP issues on external window URLs"](https://github.com/tauri-apps/tauri/issues/8476)
  and
  [tauri-apps/tauri#12740, "WebviewWindowBuilder cannot intercept external url responses/requests"](https://github.com/tauri-apps/tauri/issues/12740)
  - `on_web_resource_request` is documented as implemented only for the
    `tauri://`/`app://` internal protocol, not for external URLs. Practically:
    IPC (`window.__TAURI__`, `invoke`) does _not_ work against external pages
    by default without explicit capability/IPC-scope configuration - which is
    actually desirable here (the viewed page/snapshot should not be able to
    call back into dashboardd's Tauri commands), but it means this window type
    behaves quite differently from a widget surface and needs its own
    hardening pass (disable devtools in release, consider disabling
    JavaScript for pure snapshot viewing via the platform WebKitGTK handle
    exposed by `WebviewWindow::with_webview`, restrict navigation to the
    opened URL/host only). `tauri.conf.json`'s single global `app.security.csp`
    string applies to `WebviewUrl::App` pages; there is no per-window CSP
    override surfaced by `WebviewWindowBuilder` in this Tauri version based on
    the API used elsewhere in this codebase (only `.title/.inner_size/
.resizable/.decorations/.build()` are used) - loosening CSP for a viewer
    window must not be done by editing the one global string, or every widget
    surface's CSP loosens too.
- **Scufris control.** Would extend naturally into the same
  `Open/Update/Focus/Close` shape Scufris already drives, once the new
  command exists - `dashboardctl` symmetry is preserved for the assistant
  side. The main design question is presentation sizing (`window_dimensions`
  currently derives from widget variant width/height in "grid units",
  `service.rs:1191-1198` - an external URL surface would need its own
  sizing convention).
- **Citation → display mapping.** Direct: citation manifest's stored path
  (or its `https://` source URL for a live page) becomes the URL argument.
  No content transformation needed - full fidelity by construction.
- **Cost.** Medium. Real core dashboardd change (protocol bump, new command,
  new window path, hardening), but conceptually small and generic; the
  actual work is mostly the WebKitGTK hardening (JS-disable for snapshots,
  navigation lock, devtools-off) rather than the plumbing.

### (c) Open the real browser (firefox/chromium) with i3 window rules

Shell out to the user's real browser, and use i3 `for_window` criteria (or a
dedicated CLI wrapper) to give it a floating, sized, identifiable window.

- **Feasibility.** Already fully available - no dashboardd change of any
  kind, matches the environment description (i3 + rofi on NixOS, firefox and
  chromium present via `nix profile`: confirmed both `firefox` and
  `chromium`/`chromium-browser` resolve on `$PATH` in this environment).
- **Dedicated window class / floating / position.**
  - i3: match on `class`/`instance` via `for_window [class="..."] floating
enable, resize set ..., move position ...` - i3's own FAQ recommends
    matching class+instance over title since some apps set the title late
    ([i3 FAQ: for_window criteria](https://faq.i3wm.org/question/2172/how-do-i-find-the-criteria-for-use-with-i3-config-commands-like-for_window-eg-to-force-splashscreens-and-dialogs-to-show-in-floating-mode.1.html),
    [i3 FAQ: forcing windows floating](https://faq.i3wm.org/question/61/forcing-windows-as-always-floating.1.html)).
  - Chromium: `--class=NAME` genuinely overrides `WM_CLASS`, **but is
    ignored unless paired with `--user-data-dir=...`** because otherwise
    Chromium reuses the existing browser session/process and skips the
    class-setting code path entirely
    ([chromium issue 40172351](https://issues.chromium.org/issues/40172351)).
    So a wrapper needs a dedicated (possibly ephemeral) profile directory
    per launch, or one persistent "scufris-reference" profile. Chromium
    `--app=<url>` mode additionally gives a chromeless window (no tabs/URL
    bar) - the closest thing to a purpose-built "viewer" look without
    writing one.
  - Firefox: `--class` is unreliable across versions - Bugzilla history
    shows it not working in 3.x, and current guidance is that a _new
    window_ in an existing Firefox instance inherits the first window's
    `WM_CLASS`, so distinct classes require distinct profiles/instances
    ([Mozilla bug 496653](https://bugzilla.mozilla.org/show_bug.cgi?id=496653),
    [Arch forum thread](https://bbs.archlinux.org/viewtopic.php?id=221549)).
    Firefox is materially less scriptable than Chromium for this purpose.
- **Assistant-controlled close/focus.** Not built in, but not hard: `i3-msg`
  can focus/kill by criteria (`i3-msg '[class="scufris-reference"] focus'`,
  `kill`), and `xdotool search --class ... windowactivate|windowclose` works
  the same way for either browser. This needs a small deterministic Bash/
  Python helper per AGENTS.md conventions
  (`tools/reference/scufris-open-reference` analog to
  `tools/dashboard/scufris-dashboard`) that: launches with a fixed
  `--user-data-dir` + `--class`, records the resulting window id (from
  `xdotool search --sync --class ...` right after spawn, or by tracking the
  child PID and mapping PID→window via `xdotool search --pid`), and exposes
  open/focus/close as separate calls - i.e., hand-roll the same
  open/focus/close shape dashboardctl already gives Scufris for widgets, but
  against a real browser process instead of a dashboardd surface. This is
  real but small implementation work, entirely on the Scufris side (no
  dashboardd change, satisfies the hard constraint trivially since
  dashboardd isn't touched at all).
- **Isolation/security.** Best possible: the reference renders in an
  actual, fully up-to-date browser engine, with that browser's own sandbox,
  extension/ad-block ecosystem, PDF viewer, and no shared process/document
  with dashboardd or Scufris at all. Weakest point is exactly the opposite
  of (a)/(b): if it's a _live_ URL, this is a real browsing context with
  full JS, third-party requests, cookies, etc. - appropriate for "look at
  this web page" but not for "safely preview an unknown/untrusted saved
  snapshot" without at least an isolated/ephemeral profile
  (`--user-data-dir` again helps here: throwaway profile per open avoids
  polluting/reading the user's real browsing profile, cookies, and saved
  logins).
- **Citation → display mapping.** Simplest of all four: citation manifest's
  URL or local `file://<snapshot-path>` is literally the argument to
  `firefox`/`chromium`. No transformation, no dashboardd model to fit
  through.
- **Cost.** Low. A single small deterministic helper script plus a couple
  of `i3 config` rules the user adds once. No new product surface inside
  dashboardd.

### (d) Standalone minimal webview viewer (surf, or a small wry/tao binary)

A tiny, dedicated, single-purpose native app launched per reference, distinct
from both "real browser" and "dashboardd widget."

- **surf (suckless).** A minimal WebKit2/GTK+ browser with essentially no
  chrome, controlled via key bindings/external tools and scriptable through
  X11 window properties
  ([surf.suckless.org](https://surf.suckless.org/),
  [surf(1) manual](https://git.suckless.org/surf/file/surf.1.html)). It is
  architecturally the same engine dashboardd-desktop itself uses on Linux
  (WebKitGTK), so it renders full-fidelity HTML/CSS/JS like option (b)/(c),
  but purpose-built to look like a plain content window rather than a
  browser (no tabs/URL bar/bookmarks chrome by default). Downsides:
  configuration is compile-time (a header file, needs local patching/build
  rather than a runtime flag), and it isn't packaged in nixpkgs as
  prominently as firefox/chromium - would need a small Nix derivation. Good
  fit if the goal is "looks like a dedicated reference viewer, not a
  browser."
- **Custom wry/tao binary.** dashboardd-desktop is already Tauri (which
  wraps `wry`/`tao`) - a standalone ~100-line Rust binary using `wry`
  directly (no Tauri app shell, no IPC, no CSP entanglement, no dashboardd
  process at all) pointed at a `file://` snapshot or `https://` URL is
  straightforward and gets full engine fidelity with a from-scratch, fully
  controlled window (custom WM_CLASS via `tao`'s window builder, no
  Tauri-specific IPC surface to secure since there's no IPC at all - the
  window can be JS-enabled or JS-disabled at construction, unlike bolting a
  `WebviewUrl::External` window onto the existing multi-purpose dashboardd
  process). This avoids the CSP/IPC entanglement issues found in (b)
  entirely, because it isn't the same Tauri app. Cost is a new small
  standalone package (own build, own packaging, own lifecycle) - more
  than (c), comparable to or slightly more than (b) minus the CSP
  headaches, but it is a wholly new component to build, test, and maintain
  rather than reusing an existing browser install.
- **Isolation/security.** Same tier as (b): separate OS process, separate
  webview, no shared document with dashboardd's trusted chrome. Slightly
  better than (b) because there's no shared Tauri app/IPC bridge to
  misconfigure.
- **Scufris control.** Same shape as (c): needs a small deterministic
  open/focus/close helper using window class + `xdotool`/`i3-msg`
  (for a custom binary you also fully control the `WM_CLASS` string, so this
  is more reliable than the chromium/firefox flag situation in (c)).
- **Citation → display mapping.** Same as (b)/(c): direct URL/path argument,
  no content transformation, full fidelity.
- **Cost.** Medium (surf: patch+package a suckless app you don't own
  upstream) to medium-high (custom wry binary: new component with its own
  build/release lifecycle). Neither touches dashboardd at all, so the hard
  constraint is moot for this option.

## 3. Cross-cutting comparison

|                            | (a) viewer widget                                                                     | (b) url window in dashboardd                                              | (c) real browser + i3                                                         | (d) standalone viewer (surf/wry)                                     |
| -------------------------- | ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| dashboardd core change     | none                                                                                  | yes (new Command, new window path)                                        | none                                                                          | none                                                                 |
| Fidelity to original page  | low (sanitized, CSS mostly stripped)                                                  | full                                                                      | full                                                                          | full                                                                 |
| Isolation from trusted UI  | same-document, sanitize-only                                                          | separate webview, same app/process                                        | separate app/process                                                          | separate app/process                                                 |
| Untrusted-content safety   | strong for text/markdown/images; weak for full HTML snapshots without a big allowlist | strong by process/document separation; CSP/IPC edge cases per Tauri #8476 | strongest (real browser sandbox); use ephemeral profile for unknown snapshots | strong; least incidental attack surface (no IPC bridge at all)       |
| Scufris open/close/focus   | dashboardctl, already wired                                                           | dashboardctl, needs new command                                           | new small window-manager helper (xdotool/i3-msg)                              | new small window-manager helper (xdotool/i3-msg)                     |
| Handles live https:// URLs | no (would have to fetch+strip first)                                                  | yes                                                                       | yes                                                                           | yes                                                                  |
| Handles PDFs               | no (would need rasterization)                                                         | yes (browser PDF viewer)                                                  | yes                                                                           | surf: maybe (WebKit PDF support varies); wry: no built-in PDF viewer |
| Implementation cost        | low-medium                                                                            | medium                                                                    | low                                                                           | medium-high                                                          |

## 4. Recommendation

No single option covers every content type well; the natural split is by how
much fidelity the reference needs and how trusted its content is:

- **Article/library/den-note style references (readability-extracted text,
  markdown, screenshots of a page section)**: use **(a)**, a generic
  `dashboardd` viewer widget, following the tatr-tasks `details` variant
  pattern exactly (typed reference input → backend resolves and classifies
  → `markdown`/`text`/`image` artifact kinds → DOMPurify-sanitized render).
  This is the lowest-cost option, needs zero dashboardd core changes, and
  Scufris already has the full control-plane integration
  (`skills/dashboard/SKILL.md`, `extensions/scufris/dashboard/index.ts`,
  `tools/dashboard/scufris-dashboard`) to open/update/focus/close it exactly
  like every other widget today. It is not a good fit for full-fidelity
  HTML page snapshots (CSS/layout is largely stripped by the sanitizer) or
  PDFs.

- **"Look what I found" for a live web page, or a full-fidelity saved HTML
  snapshot where layout/CSS matters**: use **(c)**, open the real browser
  with a dedicated i3-managed window (Chromium `--app=<url> --class=...
--user-data-dir=...` for a chromeless, scriptable, ephemeral-profile
  window; a small Scufris-side helper script analogous to
  `tools/dashboard/scufris-dashboard` but wrapping `xdotool`/`i3-msg` for
  open/focus/close). This needs no dashboardd change at all, is the lowest
  implementation cost of the three full-fidelity options, and gives the
  best content isolation (a real, sandboxed, frequently-patched browser
  engine) - which matters most for content originally sourced from the open
  web.

- **If a first-class "reference surface" inside dashboardd's own window
  set is wanted later** (consistent chrome/theme with other widgets, one
  `dashboardctl` vocabulary for everything Scufris shows), **(b)** is the
  right target, but budget for the Tauri external-URL CSP/IPC rough edges
  (tauri-apps/tauri#8476, #12740) and for WebKitGTK-level hardening
  (JS-disable for pure snapshots, navigation lock, devtools-off in release)
  before treating it as trustworthy for unknown content. Prefer this over
  (d) only if "looks and behaves like the rest of dashboardd" outweighs the
  cost of extra IPC-boundary hardening.

- **(d)** is worth revisiting only if (b)'s CSP/IPC edge cases prove
  unworkable in practice, or if a genuinely chrome-free, Scufris-branded
  viewer window becomes a real product goal - it is strictly more
  implementation and maintenance cost than (c) for the same fidelity and
  isolation properties, since it means owning a new small native app instead
  of shelling out to a browser the user already has installed and trusts.

Do not attempt PDF rendering through (a); route PDFs to (c) (or, later, (b))
where the browser/webview's native PDF viewer applies.
