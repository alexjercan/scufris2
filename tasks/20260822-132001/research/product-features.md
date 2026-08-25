# Research track: sticky features and gimmicks

Agent report, 2026-08-25. Raw findings; synthesis lives in ../RESEARCH.md.

## Headline findings

Five durability laws recur across every area:

1. **Reactive beats proactive.** Everything loved (global hotkey ask, oneko, easter eggs, on-demand catch-up) waits for the user. Everything hated (Clippy, ChatGPT Pulse, sarcastic voice confirmations, Recall) initiates or captures on its own.
2. **The hotkey is the product.** The one universally praised desktop-AI affordance is instant summon from anywhere. Chat webviews get punished as "just a wrapper".
3. **Local tools are the differentiator.** MCP is what made Claude Desktop matter; Cowork proved demand for a personal-life agent and failed on security posture and no-Linux. scufris already has the winning shape.
4. **Speed decides voice adoption.** Sub-1.5s round trip plus reliable capture is the line between "family uses it" and "abandoned". Push-to-talk sidesteps the top two abandonment causes (wake misses, far-field STT).
5. **Ambient surfaces survive as dense live state, not decorative canvases.** Bars and summonable overlays are kept for years; wallpaper dashboards rot with their per-widget API config.

## 1. ChatGPT and Claude desktop apps

**Companion window + global hotkey (Option+Space / Alt+Space).** The single most consistently praised feature. HN "Why Is ChatGPT for Mac So Good?" (https://news.ycombinator.com/item?id=46105896) and the Claude Desktop thread (https://news.ycombinator.com/item?id=42007649) both credit the hotkey as the reason to keep the app over a browser tab; users filed feature requests when OpenAI moved the companion window (https://community.openai.com/t/companion-window-should-stay-available-for-chatgpt-not-only-codex/1386295). Fit: validates Super+D/Super+S as the core bet. Already shipped.

**"Just a web wrapper" backlash.** The dominant criticism of both apps is Electron bloat: 700MB vs 259MB PWA, white flashes, 5-10GB leaks in the Codex app (https://news.ycombinator.com/item?id=42007649, https://news.ycombinator.com/item?id=49281916), Gruber's "Criminally Bad Electron Mac App" (https://daringfireball.net/2026/07/claudes_criminally_bad_mac_app_is_an_inside_job). Fit: cautionary. Tauri + Kitty popup instead of a duplicated chat webview is exactly the trap avoided. Memory footprint is the metric HN actually cites.

**Screenshot of window into chat.** Quietly high-frequency and never called useless (https://help.openai.com/en/articles/9295245-chatgpt-macos-app-screenshot-tool, https://www.pocket-lint.com/chatgpt-mac-app/). Fit: strong and cheap on X11 (maim + the Pi session).

**Work with Apps / screen context.** Mixed-positive, narrow; a supporting feature, not the hook (https://help.openai.com/en/articles/10119604-work-with-apps-on-macos). Fit: the terminal equivalent (Kitty scrollback/cwd of the focused window) is far cheaper than macOS accessibility scraping.

**Voice on desktop.** Full-duplex conversational voice is a minority workflow and Advanced Voice Mode gets sour reviews. What people adopt daily is push-to-talk whisper dictation: the Whispering thread (591 points, https://news.ycombinator.com/item?id=44942731) has users saying they "can't go back to regular typing", asking for exactly a visible recording indicator, and running whisper.cpp + dotool on Linux at ~1s. One warning from that thread: "local-first" apps that quietly default to cloud STT lose trust. Fit: the pill's privacy ring and local whisper-server match the stated demand precisely.

**MCP / local tools.** The 872-point MCP thread (https://news.ycombinator.com/item?id=42237424) shows local tools are what people wanted; complaints were approval friction, config sprawl, no Linux. Cowork (https://news.ycombinator.com/item?id=46593022) proved appetite for personal-chore agents and then validated the fear with the exfiltration thread (https://news.ycombinator.com/item?id=46622328). Fit: one authoritative Pi session with narrow audited native tools is positioned exactly against Cowork's weaknesses.

## 2. Raycast

The stickiness is one muscle-memory entry point plus a few absorbed OS gaps. "Alone worth it" features: clipboard history and snippets (https://news.ycombinator.com/item?id=31868380); window management (macOS lacks a tiling WM). Quick AI as a pattern is loved ("default way to interact with AI... instead of opening a browser tab", https://albertosadde.com/blog/raycast); Raycast AI as a product is resented for subscription gating (https://news.ycombinator.com/item?id=46024754). The Linux clones (raycast-linux https://news.ycombinator.com/item?id=44551762, Vicinae https://news.ycombinator.com/item?id=45188116) show the demand is macOS-habituated developers moving to Linux.

Fit: on i3 + rofi, the launcher, clipboard UI, snippets, and window management are solved or owned by the WM - do not compete with rofi. What transfers is: instant hotkey-ask against your own agent (no subscription resentment), and a cheap "run the agent on selection/clipboard" verb. Raycast Notes' lesson - quick capture without context switch - is what the voice pill already is.

## 3. Screen capture as memory: Recall, Rewind/Limitless

**Windows Recall.** Plaintext SQLite of everything you saw; Beaumont's "two lines of code" finding forced the opt-in retreat (https://news.ycombinator.com/item?id=40543990). Sentiment stayed negative into 2025-2026 (https://www.csoonline.com/article/4159643/microsofts-windows-recall-still-allows-silent-data-extraction.html). Notably, HN did not hate the concept - several wanted it local and explicit.

**Rewind.ai.** Admired engineering (https://kevinchen.co/blog/rewind-ai-app-teardown/), but a 30-day diary review found two substantive searches per month against doubled battery drain and a "dystopian" feel (https://numericcitizen.me/rewinding-30-days-of-my-experience-with-rewind-for-mac/). An early-adopter postmortem ranks the exits: performance first, unreliable LLM recall second (https://andrewschreiber.substack.com/p/an-early-adopters-thoughts-on-rewindais). The company itself abandoned desktop screen memory for the Limitless pendant, then sold to Meta (https://9to5mac.com/2025/12/05/rewind-limitless-meta-acquisition/). Screenpipe repositioned to agent-context plumbing; a user reported 90%+ of 1,000 recorded hours was "boring nothingness" (https://news.ycombinator.com/item?id=41695840).

**What retains:** ActivityWatch-class window-title journals and explicit curated memory, not pixels (https://activitywatch.net/). Fit: no always-on capture. The retained granularity is an explicit "remember this" verb plus queryable activity summaries.

## 4. Home Assistant Assist voice satellites

**What sticks:** timers above everything (third most requested feature, shipped as the headline of Voice chapter 7, https://www.home-assistant.io/blog/2024/06/26/voice-chapter-7/; confirmed as the retained daily use in the 15-month review, https://botmonster.com/smart-home/home-assistant-voice-preview-edition-review/), room-scoped device control, targeted spoken announcements, LED state signaling. Users prefer a short affirmative sound over verbose TTS (https://news.ycombinator.com/item?id=47398534).

**What kills adoption:** wake-word misses (20-25% rejection near a range hood; ~30% success for women and children on volunteer-trained models), slow general Q&A (3.89s average local response, 37% recognition in one build, partner verdict "I still miss just being able to ask questions", https://www.joekarlsson.com/blog/i-replaced-my-smart-home-with-a-dumber-home-but-at-least-its-private/), rigid phrasing. Abandonment quote: "I spend a lot of time updating it and NO time using it" (https://community.home-assistant.io/t/home-assistant-voice-preview-edition-good-substitute-for-alexa/1011846).

**Validated architecture:** prefer-local deterministic intents with LLM fallback (https://community.home-assistant.io/t/voice-assistant-fall-back-to-an-llm-based-agent/834798); sub-1.5s for simple actions is the adoption threshold. Follow-up without re-wake was demanded for years and shipped in chapter 10: if the reply is a question, the mic reopens (https://www.home-assistant.io/blog/2025/06/25/voice-chapter-10/). Known HA gap: the local fast path is bypassed mid-conversation (https://community.home-assistant.io/t/prefer-handling-commands-locally-during-continued-conversation/883012).

Fit: scufris-desktop inverts HA's weaknesses. Push-to-talk is the wake word; the desk mic is 50cm away; the Pi agent supplies the general Q&A that HA users mourn. Timers held in the agent session and rendered on the HUD fix HA's "device-local timer" complaint for free.

## 5. Ambient/HUD precedents

**What people keep for 15+ years:** clocks/timezones, task lists, weather, system vitals, now-playing - self-updating, zero-config content, at bar/overlay scale (polybar/i3blocks module sets; GNOME Vitals/OpenWeather, https://www.omgubuntu.co.uk/best-gnome-shell-extensions). Conky abandoners are explicit: "spent more time customizing things than getting work done". Reference material (keybind cheatsheets) is wanted on demand, never persistent (https://github.com/regolith-linux/regolith-desktop/issues/89).

**What churns:** decorative widget canvases. eww's own author positions it for summonable overlay dashboards, not background info, and a three-month user retreated to i3status-rust over instability (https://news.ycombinator.com/item?id=35700963). Glance and TRMNL prove the briefing-surface want is real (https://news.ycombinator.com/item?id=40357611, https://news.ycombinator.com/item?id=42137513) - and both die by config/API rot ("I'll get it set up just so, then some api changes").

**Stream Deck** (https://news.ycombinator.com/item?id=31528895): what sticks is stateful one-tap verbs with global targets - meeting mute, status switching, HA lights with live sensor values on the buttons. Generic app launching gets abandoned. The load-bearing insight: buttons display live state, so nothing is memorized.

**AI in ambient widgets:** the pieces exist separately (ai-usagebar waybar widget, https://github.com/akitaonrails/ai-usagebar; briefing generators; TRMNL playlists) but no incumbent renders agent-maintained content as a Linux desktop HUD. This intersection is genuinely unclaimed - and it is exactly the v3 dashboardd-embed position. scufris's structural edge over Glance/TRMNL: the agent maintains the data plumbing, so widgets never carry per-widget API config.

## 6. Presence gimmicks

**Clippy failed on interruption, not character.** Stanford's Reeves: "the worst thing about Clippy was that he interrupted" (https://thenewstack.io/humanity-vs-clippy-lessons-from-microsofts-failed-virtual-assistant/). The 2025 local-LLM Clippy revival hit the HN front page on nostalgia but nobody planned daily use (https://news.ycombinator.com/item?id=43905942).

**Avatar bodies decay in an hour.** Desktop Mate sits at Mixed on Steam; "there isn't really much reason to keep booting this up other than it being interesting for the first hour" (https://steamcommunity.com/app/3301060/discussions/1/594011910715770103/); the review bombing came from removing mod support - attachment forms around customization, not the shipped character.

**The useless cat doctrine.** oneko has been ported continuously since 1989 (https://github.com/glreno/oneko) - the strongest longevity signal found. It survives because it costs zero attention and reacts to you rather than demanding reaction. A reactive state animation tied to real agent state (sleeping/alert/working) inherits oneko's durability while smuggling in utility no pure pet has.

**Voice personality:** loved to build, disabled in use. GLaDOS builds get stars (https://github.com/dnhkng/GLaDOS); sarcastic content in confirmations gets reverted "by the third time" (https://www.techradar.com/computing/artificial-intelligence/i-tried-chatgpts-new-sarcastic-voice-and-it-made-me-hate-mondays-even-more). Character belongs in the voice timbre, not in extra words.

**Earcons:** Alexa Brief Mode exists because users tire of spoken confirmations - chime the acks, speak only answers (https://techcrunch.com/2018/03/16/alexas-new-brief-mode-replaces-verbal-confirmations-with-chimes). One sound heard 50x/day must be near-subliminal or it gets muted.

**Easter eggs and seasonal decoration:** pull-only delight, near-zero cost, decades of retention in terminal culture (sl, fortune | cowsay, https://hamvocke.com/blog/commandline-fun/). Season the decorations, never the functional signals.

**Neuro-sama's one transferable law** (CHI 2026 study, https://arxiv.org/html/2509.10427v1): attachment anchors on persona consistency. One stable name, voice, and temperament; no rotating personalities.

## 7. Proactive patterns

The graveyard is large: Alexa Flash Briefing decayed to 18-21% daily use and quiet death (https://stoprewindplay.substack.com/p/what-podcasters-can-learn-from-flash); Google Snapshot was killed after polls showed nobody used it (https://www.androidpolice.com/google-is-killing-snapshot-the-now-replacement-that-never-took-off/); ChatGPT Pulse's HN reception was "feature nobody asked for" plus privacy alarm - with a minority saying it would only be acceptable local and privacy-preserving (https://news.ycombinator.com/item?id=45375477). Apple's AI notification summaries had to be partially disabled after fabricated headlines; the batching mechanism underneath is liked (https://www.howtogeek.com/why-and-how-i-turned-off-apple-intelligence-notification-summaries-on-iphone-and-mac/).

Kept nudges share five properties: user-authored trigger, actionability, precision over coverage (Google Now's "leave now" cards are still mourned), citations to ground truth (Slack recaps), delivery at a chosen batch time. Fit: being local removes the privacy objection but not the noise objection. Precision is the product.

## Ranked top-8 for scufris-desktop

Ranked by expected daily-retained value against the "big reason to leave the terminal" bar, tempered by evidence strength.

**1. Dictation-everywhere: pill mode that types into the focused window. (small)**
A second pill mode: transcript goes to the focused X11 window (xdotool type) instead of the agent. The 591-point Whispering thread is the strongest daily-habit evidence in this study - "can't go back to regular typing" - and the pill already owns capture, STT, review, and focus restoration. Clears the bar by inversion: it does not pull the owner out of the terminal, it makes the desktop layer indispensable inside it, so Super+D becomes hourly muscle memory that everything else rides on.

**2. Fast-verb tier with agent fallback, led by timers on the HUD. (medium)**
A deterministic intent matcher in the daemon for a tiny verb set - timers/reminders, open/focus, mute, "brief me" - answered in under 1.5s with an earcon, everything else falling through to the agent. Timers are the single most proven voice feature in existence, and agent-authoritative timers rendered as a dashboardd countdown widget fix HA's top timer complaint. Clears the bar: a terminal has no ambient countdown, and voice-set timers while hands are on the keyboard is the one voice act everyone retains. First concrete content for the widget host.

**3. "Look at this": window and scrollback context capture. (small)**
One verb that snapshots the focused window (maim) or, for Kitty windows, grabs scrollback and cwd as text, into the authoritative session. Screenshot-into-chat is the quietly high-frequency winner of both vendor apps; the text path is cheaper and better than their accessibility scraping. Clears the bar: it removes the copy-paste loop between the terminal and the assistant, which is the actual friction today.

**4. The summonable HUD: dashboardd embed as overlay, not wallpaper. (large)**
The v3 dashboardd-runtime embed, shaped by the evidence: a hotkey-summoned, glanceable overlay (eww's own positioning; Regolith's hold-to-show cheatsheet demand) holding agent-fed widgets - briefing, timers, den references, and a Stream Deck-style row of stateful action tiles wired to skills. Never an always-on canvas; zero per-widget config, because the agent maintains the feed - the structural edge over Glance/TRMNL, whose documented killer is config rot. Clears the bar directly: interactive, agent-maintained visual state is the one thing the terminal genuinely cannot render, and the intersection is unclaimed on Linux.

**5. Morning briefing, done the retained way. (medium)**
User-authored systemd timer, only actionable items, every line linked to its source (widget or den item on the HUD), one spoken sentence at most, per-topic mute from day one. Also ship the pull twin - "catch me up" on demand - since pull is the shape with the better retention record. Serves ambient glanceability more than the bar; it is the calibration ground for all future proactivity.

**6. Explicit memory: "remember this" plus on-demand recall. (medium)**
A voice/pill verb that files a fact or the current context capture into the den, and a recall verb over it. The Recall/Rewind evidence says explicit curated memory is the granularity people keep; continuous capture is the one they uninstall. Optional, evidence-gated: query an existing ActivityWatch instance for "what was I doing Tuesday" rather than building any capture. Clears the bar: the terminal has no cross-session personal memory; the agent plus den does.

**7. Turn-taking and the speak/chime split. (small)**
When the agent's reply is a question, the pill reopens the mic without re-activation (HA chapter 10's most-demanded fix); routine acks become earcons, only answers are spoken (Alexa Brief Mode). Two small rules that the evidence says decide whether voice survives week two. Pure retention insurance for features 1-2.

**8. The presence layer: reactive state animation, earcons, easter eggs, one persona. (small)**
An oneko-inspired reactive tray/pill animation bound to real assistant state (idle/listening/working/attention/error already exist in the daemon), a coherent themed earcon set, a few hidden pill responses and seasonal tray variants, and one permanent persona with a distinctive local Piper timbre and a snark budget of zero words in confirmations. Individually tiny; together they are what makes Scufris feel like an inhabitant rather than a program - the oneko evidence says reactive presence is kept for decades. Never initiates. Cheapest durable delight per line of code in the whole list.

**Explicitly not worth adopting:** a chat webview (Electron backlash), launcher/clipboard/snippets/window management (rofi and i3 own them; Raycast wins only where the OS has gaps), avatar bodies (Desktop Mate's one-hour ceiling), continuous screen recording (Rewind's own pivot is the verdict), sarcastic personality content, assistant-initiated speech outside the briefing budget, and wake word before the hotkey path saturates.
