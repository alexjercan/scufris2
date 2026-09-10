// The conversation window: every line the service has pushed, and a field to
// add one from.
//
// This page renders and reports, the way the pill and the textbox pages do.
// Every decision is the Rust side's: src/conversation.rs decides what a typed
// line means and src/hud.rs decides what the window does. What arrives here is
// lines to draw and a notice to show; what leaves is Enter and Escape.
//
// Required response text is literal plain prose. Details alone are safe
// Markdown. Both can make bare HTTP and HTTPS URLs actionable, but neither can
// create executable HTML. The one transient `thinking...` row is presentation
// of service state and never enters history.
//
// Wrapped in a block: the pages are separate classic scripts in one tsc
// project, so a name at the top level of one is a name in the others' global
// scope.
//
// Compiled by tsc from build.rs into ui/dist; the window loads the output.

{
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;

  const forward = (level: string, message: string): void => {
    try {
      invoke("pill_log", { level: `hud.${level}`, message }).catch(() => {});
    } catch {
      // Nothing to do: the log stays in the webview console only.
    }
  };

  window.addEventListener("error", (event) => {
    forward("error", `uncaught: ${event.message}`);
  });

  window.addEventListener("unhandledrejection", () => {
    forward("error", "unhandled rejection");
  });

  const element = <T extends HTMLElement>(id: string): T => {
    const found = document.getElementById(id);
    if (found === null) throw new Error(`the HUD page is missing #${id}`);
    return found as T;
  };

  const lines = element<HTMLOListElement>("lines");
  const briefings = element<HTMLLIElement>("briefings");
  const briefingRows = element<HTMLElement>("briefing-rows");
  const jobs = element<HTMLLIElement>("jobs");
  const rows = element<HTMLElement>("rows");
  const notice = element<HTMLElement>("notice");
  const words = element<HTMLTextAreaElement>("words");
  const selected = element<HTMLElement>("selected");
  const attach = element<HTMLButtonElement>("attach");
  const latest = element<HTMLButtonElement>("latest");
  const fresh = element<HTMLElement>("fresh");

  // What the gutter says for each speaker. A speaker with no word here is one
  // this build does not know about, and it is drawn rather than dropped: a line
  // that was said belongs on screen whoever the service says said it.
  const WHO: Record<string, string> = {
    user: "you",
    assistant: "scufris",
  };

  /** What the row's state column says. Four words, one column wide. */
  const STATE_WORDS: Record<string, string> = {
    working: "work",
    blocked: "block",
    done: "done",
    failed: "fail",
  };

  const BRIEFING_WORDS: Record<string, string> = {
    collecting: "gather",
    collected: "ready",
    failed: "fail",
    pending: "ready",
    in_progress: "write",
    delivered: "done",
  };

  /** What one line does to the notice line, when nothing is in flight. */
  const KEYS = "enter sends - + attaches - esc closes";
  /**
   * How long a stop stays armed before it forgets it was pressed.
   *
   * Stopping the wrong job costs an hour of an agent's work, so the control
   * asks once. Three seconds is long enough to answer and short enough that
   * an armed button is never still armed when the pointer comes back.
   */
  const ARMED_MS = 3000;
  /**
   * How near the bottom still counts as reading the newest line.
   *
   * A person is at the bottom of a conversation long before they are at the
   * last pixel of it: a window that only followed an exact bottom would stop
   * following the moment a line wrapped one row further than the last one did.
   */
  const NEAR = 24;
  let thinkingLine: HTMLLIElement | null = null;
  /** The stop control waiting for its second press, if one is. */
  let armed: { button: HTMLButtonElement; timer: number } | null = null;
  /** Every drawn offer, by identifier, so a spent one can be found again. */
  const drawnOffers = new Map<string, HTMLButtonElement[]>();
  let selectedAttachments: AttachmentDescriptor[] = [];
  /** True while the window is to keep the newest line in view. */
  let following = true;
  /** Lines that have arrived since the reader stopped following. */
  let unseen = 0;

  const size = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${Math.ceil(bytes / 1024)} KiB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
  };

  const presentationMediaType = (descriptor: AttachmentDescriptor): string => {
    if (descriptor.media_type !== "application/octet-stream")
      return descriptor.media_type;
    const extension = descriptor.name.split(".").pop()?.toLowerCase();
    const videoTypes: Record<string, string> = {
      m4v: "video/x-m4v",
      mkv: "video/x-matroska",
      mov: "video/quicktime",
      mp4: "video/mp4",
      webm: "video/webm",
    };
    return extension === undefined
      ? descriptor.media_type
      : (videoTypes[extension] ?? descriptor.media_type);
  };

  const hasInlineImage = (descriptor: AttachmentDescriptor): boolean => {
    const mediaType = presentationMediaType(descriptor);
    return mediaType.startsWith("image/") && mediaType !== "image/svg+xml";
  };

  const hasInlineVideo = (descriptor: AttachmentDescriptor): boolean =>
    presentationMediaType(descriptor).startsWith("video/");

  const action = (
    label: string,
    title: string,
    run: () => Promise<unknown>,
  ): HTMLButtonElement => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "attachment-action";
    button.textContent = label;
    button.title = title;
    button.addEventListener("click", (event) => {
      event.stopPropagation();
      void run().catch((error: unknown) => {
        notice.dataset["tone"] = "trouble";
        notice.textContent = String(error);
      });
    });
    return button;
  };

  const openLink = (url: string): void => {
    void invoke("hud_open_link", { url }).catch(() => {
      notice.dataset["tone"] = "trouble";
      notice.textContent = "The link could not be opened.";
    });
  };

  // ---------- following the newest line ----------

  /** True while the person is reading the bottom of the conversation. */
  const atBottom = (): boolean =>
    lines.scrollHeight - lines.scrollTop - lines.clientHeight < NEAR;

  /** Puts the newest line back in view. */
  const pin = (): void => {
    lines.scrollTop = lines.scrollHeight;
  };

  /** Shows the way back, and says how much is waiting at the end of it. */
  const drawLatest = (): void => {
    latest.hidden = following;
    if (unseen > 0) latest.dataset["unseen"] = "yes";
    else delete latest.dataset["unseen"];
    const waiting =
      unseen === 0 ? "" : unseen === 1 ? ", 1 new" : `, ${unseen} new`;
    latest.setAttribute("aria-label", `Jump to the latest message${waiting}`);
    // Said once, when the conversation moves on without the reader. Repeating
    // it for every line that follows would talk over what they are reading.
    fresh.textContent =
      unseen === 0
        ? ""
        : unseen === 1
          ? "1 new message below"
          : `${unseen} new messages below`;
  };

  /**
   * Reads where the person is and settles what follows from it.
   *
   * The position is measured rather than remembered. A wheel, a drag, a key,
   * the field growing under the list and the window putting a line back all
   * move it, and the only account of it that cannot go stale is the scroller's
   * own.
   */
  const settle = (): void => {
    following = atBottom();
    if (following) unseen = 0;
    drawLatest();
  };

  // ---------- drawing ----------

  /**
   * Who said the last line of the conversation, ignoring the thinking row.
   *
   * The thinking row is presentation and never part of what was said, so a
   * line that arrives under it is a reply to the line above it.
   */
  const lastSpeaker = (): string | null => {
    const drawn = lines.children;
    for (let index = drawn.length - 1; index >= 0; index -= 1) {
      const line = drawn[index] as HTMLElement | undefined;
      if (line === undefined) continue;
      if (line.dataset["transient"] !== undefined) continue;
      return line.dataset["speaker"] ?? null;
    }
    return null;
  };

  /**
   * Says what the space above a line is for: a change of speaker, or one more
   * thing from whoever said the line above it.
   */
  const mark = (line: HTMLElement, before: string | null): void => {
    line.dataset["run"] =
      before !== null && before === line.dataset["speaker"]
        ? "continued"
        : "new";
  };

  const draw = (
    entry: ConversationEntry,
    before: string | null,
  ): HTMLLIElement => {
    const line = document.createElement("li");
    line.className = "line";
    line.dataset["speaker"] = entry.role;
    mark(line, before);

    const who = document.createElement("span");
    who.className = "who";
    who.textContent = WHO[entry.role] ?? entry.role;

    const what = document.createElement("span");
    what.className = "what";
    window.scufrisMarkup.renderPlain(what, entry.text, openLink);
    line.append(who, what);
    if (entry.attachments && entry.attachments.length > 0) {
      const attachments = document.createElement("span");
      attachments.className = "message-attachments";
      for (const descriptor of entry.attachments) {
        const item = document.createElement("span");
        item.className = "message-attachment";
        if (hasInlineImage(descriptor) || hasInlineVideo(descriptor)) {
          const preview = action("", `Preview ${descriptor.name}`, () =>
            invoke("hud_open_attachment", { descriptor }),
          );
          preview.className = "attachment-thumbnail";
          const image = document.createElement("img");
          image.className = "attachment-preview";
          image.src = `scufris-attachment://content/${descriptor.id}`;
          image.alt = descriptor.name;
          image.loading = "lazy";
          preview.append(image);
          if (hasInlineVideo(descriptor)) {
            const play = document.createElement("span");
            play.className = "attachment-play";
            play.textContent = "play";
            preview.append(play);
          }
          item.append(preview);
        }
        const identity = document.createElement("span");
        identity.className = "attachment-identity";
        const name = document.createElement("strong");
        name.textContent = descriptor.name;
        const metadata = document.createElement("small");
        metadata.textContent = `${presentationMediaType(descriptor)} - ${size(descriptor.size)}`;
        identity.append(name, metadata);
        item.append(identity);
        item.append(
          action("save", `Save ${descriptor.name}`, () =>
            invoke("hud_save_attachment", { descriptor }),
          ),
        );
        attachments.append(item);
      }
      line.append(attachments);
    }
    if (entry.details) {
      const details = document.createElement("div");
      details.className = "details";
      details.setAttribute("aria-label", "Response details");
      window.scufrisMarkup.renderDetails(details, entry.details, openLink);
      line.append(details);
    }
    // The badges go at the foot of the message, one rank per job, led by the
    // job's own id. Nothing here reads the prose above them.
    for (const citation of entry.receipts ?? []) line.append(strip(citation));
    return line;
  };

  /** One job's badges, as the rank drawn under the message that cites it. */
  const strip = (citation: Citation): HTMLElement => {
    const rank = document.createElement("span");
    rank.className = "strip";
    const cite = document.createElement("span");
    cite.className = "cite";
    cite.textContent = citation.job_id;
    rank.append(cite);
    for (const badge of citation.badges ?? []) {
      const mark = document.createElement("span");
      mark.className = "badge";
      mark.dataset["state"] = badge.state;
      const label = document.createElement("span");
      label.className = "badge-label";
      label.textContent = badge.label;
      mark.append(label, document.createTextNode(badge.value));
      rank.append(mark);
    }
    for (const offer of citation.offers ?? []) rank.append(drawOffer(offer));
    return rank;
  };

  /**
   * One offer, as the control it is.
   *
   * The words behind it never reached this page. A press sends the identifier
   * and the extension runs what it stored against it, so no button in this
   * window can put a sentence into the conversation.
   */
  const drawOffer = (offer: Offer): HTMLButtonElement => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "offer";
    button.textContent = offer.label;
    button.dataset["offer"] = offer.id;
    if (offer.taken) button.dataset["spent"] = "";
    drawnOffers.set(offer.id, [...(drawnOffers.get(offer.id) ?? []), button]);
    button.addEventListener("click", () => {
      if (button.dataset["spent"] !== undefined || button.disabled) return;
      button.disabled = true;
      // Spent is the service's to say. It answers with offer-taken, which is
      // what marks every drawing of this offer, the replay included.
      void invoke("hud_offer_take", { id: offer.id }).catch(
        (error: unknown) => {
          button.disabled = false;
          notice.dataset["tone"] = "trouble";
          notice.textContent = String(error);
        },
      );
    });
    return button;
  };

  /** Marks one offer spent wherever it is drawn. */
  const spend = (id: string): void => {
    for (const button of drawnOffers.get(id) ?? []) {
      button.dataset["spent"] = "";
      button.disabled = false;
    }
  };

  // ---------- the job list ----------

  /** How old the row is, in the largest unit that still says something. */
  const age = (since: number): string => {
    const seconds = Math.max(0, Math.floor(Date.now() / 1000) - since);
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h`;
    return `${Math.floor(seconds / 86400)}d`;
  };

  const disarm = (): void => {
    if (armed === null) return;
    window.clearTimeout(armed.timer);
    delete armed.button.dataset["armed"];
    armed.button.textContent = "x";
    armed = null;
  };

  const command = (id: string, act: "cancel" | "archive"): void => {
    void invoke("hud_job_command", { id, action: act }).catch(
      (error: unknown) => {
        notice.dataset["tone"] = "trouble";
        notice.textContent = String(error);
      },
    );
  };

  /**
   * The one control a row has, and which one it is says what the row is.
   *
   * A live job can only be stopped and a finished one can only be filed.
   * Stopping arms first: one press asks, a second inside three seconds does
   * it. Stopping the wrong job costs an hour of an agent's work.
   */
  const rowControl = (row: JobRow): HTMLButtonElement => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "row-do";
    const terminal = row.state === "done" || row.state === "failed";
    button.dataset["act"] = terminal ? "archive" : "cancel";
    button.textContent = terminal ? "clear" : "x";
    button.title = terminal
      ? `File job ${row.id}`
      : `Stop job ${row.id}. Press twice.`;
    button.addEventListener("click", () => {
      if (terminal) {
        command(row.id, "archive");
        return;
      }
      if (armed?.button === button) {
        disarm();
        command(row.id, "cancel");
        return;
      }
      disarm();
      button.dataset["armed"] = "";
      button.textContent = "sure?";
      armed = { button, timer: window.setTimeout(disarm, ARMED_MS) };
    });
    return button;
  };

  const drawRow = (row: JobRow): HTMLElement => {
    const drawn = document.createElement("div");
    drawn.className = "row";
    drawn.dataset["state"] = row.state;
    const identity = document.createElement("span");
    identity.className = "row-id";
    identity.textContent = row.id;
    const project = document.createElement("span");
    project.className = "row-project";
    project.textContent = row.project ?? "";
    project.title = row.project ?? "";
    const state = document.createElement("span");
    state.className = "row-state";
    state.textContent = STATE_WORDS[row.state] ?? row.state;
    const since = document.createElement("span");
    since.textContent = age(row.since);
    const said = document.createElement("span");
    said.className = "row-said";
    said.textContent = row.summary;
    said.title = row.summary;
    drawn.append(identity, project, state, since, said, rowControl(row));
    return drawn;
  };

  /**
   * Draws every row, and the way to file the finished ones at once.
   *
   * A row outlives its job: finishing does not remove it, so last night's run
   * is still here this morning. Filing it is the acknowledgement, and is the
   * only thing that clears it.
   */
  const list = (listed: JobRow[]): void => {
    disarm();
    const drawn = listed.map(drawRow);
    const finished = listed.filter(
      (row) => row.state === "done" || row.state === "failed",
    );
    if (finished.length >= 2) {
      const sweep = document.createElement("button");
      sweep.type = "button";
      sweep.className = "sweep";
      sweep.textContent = `clear the ${finished.length} finished`;
      sweep.addEventListener("click", () => {
        for (const row of finished) command(row.id, "archive");
      });
      drawn.push(sweep);
    }
    rows.replaceChildren(...drawn);
    tail();
  };

  /** Draws durable scheduled briefing state. It has no controls: collection
   * and delivery ownership stay outside every foreground surface. */
  const listBriefings = (listed: BriefingRow[]): void => {
    briefingRows.replaceChildren(
      ...listed.map((row) => {
        const drawn = document.createElement("div");
        drawn.className = "briefing-row";
        drawn.dataset["state"] =
          row.delivery === "delivered" ? "delivered" : row.collection;
        const profile = document.createElement("span");
        profile.className = "briefing-profile";
        profile.textContent = row.profile;
        profile.title = `${row.profile} for ${row.date}`;
        const state = document.createElement("span");
        state.className = "briefing-state";
        const key =
          row.delivery === "in_progress" || row.delivery === "delivered"
            ? row.delivery
            : row.delivery === "pending" && row.collection !== "collecting"
              ? "pending"
              : row.collection;
        state.textContent = BRIEFING_WORDS[key] ?? key;
        const count = document.createElement("span");
        count.className = "briefing-count";
        count.textContent = `${row.completed}/${row.total}`;
        const summary = document.createElement("span");
        summary.className = "briefing-summary";
        summary.textContent = row.summary;
        summary.title = row.summary;
        drawn.setAttribute(
          "aria-label",
          `${row.profile} briefing for ${row.date}, ${state.textContent}, ${row.completed} of ${row.total} sources: ${row.summary}`,
        );
        drawn.append(profile, state, count, summary);
        return drawn;
      }),
    );
    tail();
  };

  /** Keeps quiet lifecycle lists at the end of the conversation flow. */
  const tail = (): void => {
    briefings.remove();
    jobs.remove();
    if (briefingRows.children.length > 0) {
      briefings.hidden = false;
      lines.append(briefings);
    }
    if (rows.children.length > 0) {
      jobs.hidden = false;
      lines.append(jobs);
    }
  };

  const append = (entry: ConversationEntry): void => {
    if (entry.role === "assistant") setThinking(false);
    // Whether to follow is decided before the line goes in, because adding it
    // is what changes the answer. A person who has scrolled up to read
    // something is not dragged back down by the next line arriving; they are
    // told there is something under them instead.
    following = atBottom();
    const line = draw(entry, lastSpeaker());
    // The thinking row is the reply that has not arrived yet, so it stays
    // under everything that has. A line said while Scufris is working goes
    // above it rather than after it.
    if (thinkingLine === null) {
      lines.append(line);
    } else {
      lines.insertBefore(line, thinkingLine);
      mark(thinkingLine, entry.role);
    }
    tail();
    if (following) pin();
    else unseen += 1;
    drawLatest();
  };

  const replace = (entries: ConversationEntry[]): void => {
    thinkingLine = null;
    drawnOffers.clear();
    let before: string | null = null;
    const drawn = entries.map((entry) => {
      const line = draw(entry, before);
      before = entry.role;
      return line;
    });
    lines.replaceChildren(...drawn);
    tail();
    // A whole conversation arriving is the service replaying itself, which
    // there is no reading position in: what was under the reader is gone.
    following = true;
    unseen = 0;
    pin();
    drawLatest();
  };

  const setThinking = (active: boolean): void => {
    if (!active) {
      thinkingLine?.remove();
      thinkingLine = null;
      return;
    }
    const follow = atBottom();
    if (thinkingLine === null) {
      thinkingLine = draw(
        {
          role: "assistant",
          surface: "presentation",
          text: "thinking...",
        },
        lastSpeaker(),
      );
      thinkingLine.dataset["transient"] = "thinking";
      lines.append(thinkingLine);
      tail();
    }
    if (follow) pin();
  };

  const drawSelected = (attachments: AttachmentDescriptor[]): void => {
    selectedAttachments = attachments;
    selected.replaceChildren(
      ...attachments.map((descriptor) => {
        const chip = document.createElement("span");
        chip.className = "selected-attachment";
        const name = document.createElement("span");
        name.textContent = `${descriptor.name} - ${size(descriptor.size)}`;
        const remove = action("x", `Remove ${descriptor.name}`, async () => {
          const state = (await invoke("hud_detach", {
            id: descriptor.id,
          })) as Notice;
          say(state);
        });
        remove.classList.add("attachment-remove");
        chip.append(name, remove);
        return chip;
      }),
    );
  };

  const say = (state: Notice): void => {
    setThinking(state.thinking === true);
    drawSelected(state.attachments ?? []);
    if (state.trouble !== "") {
      notice.dataset["tone"] = "trouble";
      notice.textContent = state.trouble;
      return;
    }
    if (state.sending) {
      notice.dataset["tone"] = "sending";
      notice.textContent = "sending";
      return;
    }
    notice.dataset["tone"] = "keys";
    notice.textContent = KEYS;
  };

  // ---------- the field ----------

  // The window cannot be resized once it is up, so the field grows into the
  // room the conversation is using and stops. Measured rather than counted:
  // the type is monospace by preference and not by guarantee, and it wraps.
  const fit = (): void => {
    words.style.height = "auto";
    words.style.height = `${words.scrollHeight}px`;
  };

  words.addEventListener("input", fit);

  // ---------- the way back ----------

  lines.addEventListener("scroll", settle);

  latest.addEventListener("click", () => {
    pin();
    settle();
    // The control is about to leave the page, and the keyboard cannot stay on
    // an element that is not there. It goes back where this window keeps it.
    words.focus();
  });

  // An attachment thumbnail is a picture that arrives after the line holding
  // it, and the conversation gets taller under whoever is reading it. Load
  // events do not bubble, so this listens on the way down.
  lines.addEventListener(
    "load",
    () => {
      if (following) pin();
    },
    true,
  );

  // The window cannot be resized, but the scroller can: the field grows as it
  // is typed into and the selected attachments appear above it. Whoever was
  // reading the newest line is still reading it afterwards.
  if (typeof ResizeObserver !== "undefined") {
    const watch = new ResizeObserver(() => {
      if (following) pin();
      settle();
    });
    watch.observe(lines);
  }

  attach.addEventListener("click", () => {
    if (attach.disabled) return;
    if (selectedAttachments.length >= 8) {
      // The button still looks live - `disabled` is set only while an import
      // runs - so a click that returns here is a paperclip that does nothing
      // and says nothing. The host wrote this sentence for exactly this and
      // the guard made it unreachable.
      notice.dataset["tone"] = "trouble";
      notice.textContent =
        "A message can contain at most 8 different attachments.";
      return;
    }
    attach.disabled = true;
    notice.dataset["tone"] = "sending";
    notice.textContent = "importing";
    void invoke("hud_attach")
      .then((state) => say(state as Notice))
      .catch((error: unknown) => {
        notice.dataset["tone"] = "trouble";
        notice.textContent = String(error);
      })
      .finally(() => {
        attach.disabled = false;
      });
  });

  const send = async (): Promise<void> => {
    const text = words.value;
    if (text.trim() === "") return;
    // Cleared on the host taking the line, and on nothing else. Not when the
    // service answers - the person has moved on to the next thing they want to
    // say, and a field that stayed full for a socket round trip is one they
    // would type over. But not before asking either: a second Enter while a
    // line is in flight is refused, and the whole reason refusing is acceptable
    // rather than queueing is that the words stay in the field. Clearing first
    // meant they did not, and nothing came back to say the sentence was gone.
    //
    // This wait is the host deciding, which is one IPC hop, not the service
    // answering.
    const taken = (await invoke("hud_submit", { text })) as boolean;
    if (!taken) return;
    words.value = "";
    selectedAttachments = [];
    selected.replaceChildren();
    fit();
  };

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void invoke("hud_close");
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      // A control that has the keyboard answers Enter itself. Sending from
      // under it is how the attach control, the save on an attachment and the
      // way back to the newest line were all unreachable without a mouse.
      const owner = document.activeElement as HTMLElement | null;
      if (owner !== null && owner.tagName === "BUTTON") return;
      // Shift+Enter is the newline. One Enter is one message, which is the
      // same bargain the textbox makes.
      event.preventDefault();
      void send();
    }
  });

  // A window manager can hand this window the keyboard without the field
  // taking it - a click on the border, a focus-follows-mouse enter. The keys
  // are the field's.
  window.addEventListener("focus", () => {
    if (document.activeElement === words) return;
    words.focus();
  });

  // ---------- what the host says ----------

  void listen("scufris://said", (event) => {
    append(event.payload as ConversationEntry);
  });

  // The service replays its whole transcript ring to a frontend that connects,
  // so a reconnection is a whole conversation arriving rather than a line.
  void listen("scufris://conversation", (event) => {
    const backlog = event.payload as Backlog;
    replace(backlog.lines);
    listBriefings(backlog.briefings ?? []);
    list(backlog.jobs ?? []);
    say(backlog.notice);
  });

  void listen("scufris://notice", (event) => {
    say(event.payload as Notice);
  });

  void listen("scufris://jobs", (event) => {
    const follow = atBottom();
    list(event.payload as JobRow[]);
    if (follow) pin();
    settle();
  });

  void listen("scufris://briefings", (event) => {
    const follow = atBottom();
    listBriefings(event.payload as BriefingRow[]);
    if (follow) pin();
    settle();
  });

  void listen("scufris://offer-taken", (event) => {
    spend(event.payload as string);
  });

  // The window is built at startup and filled whether it is on screen or not,
  // so there is usually a backlog by the time anybody opens it.
  void (async () => {
    const backlog = (await invoke("hud_ready")) as Backlog;
    replace(backlog.lines);
    listBriefings(backlog.briefings ?? []);
    list(backlog.jobs ?? []);
    say(backlog.notice);
    fit();
    words.focus();
  })();
}
