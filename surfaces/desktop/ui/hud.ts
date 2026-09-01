// The conversation window: every line the service has pushed, and a field to
// add one from.
//
// This page renders and reports, the way the pill and the textbox pages do.
// Every decision is the Rust side's: src/conversation.rs decides what a typed
// line means and src/hud.rs decides what the window does. What arrives here is
// lines to draw and a notice to show; what leaves is Enter and Escape.
//
// The lines it draws are text. Not markdown, not tool calls, not thinking - the
// service's conversation projection is what was said. The one transient
// `thinking...` row is presentation of service state and never enters history.
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

  /** What one line does to the notice line, when nothing is in flight. */
  const KEYS = "enter sends - + attaches - esc closes";
  /**
   * How near the bottom still counts as reading the newest line.
   *
   * A person is at the bottom of a conversation long before they are at the
   * last pixel of it: a window that only followed an exact bottom would stop
   * following the moment a line wrapped one row further than the last one did.
   */
  const NEAR = 24;
  let thinkingLine: HTMLLIElement | null = null;
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
    what.textContent = entry.text;
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
      const details = document.createElement("pre");
      details.className = "details";
      details.textContent = entry.details;
      line.append(details);
    }
    return line;
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
    if (following) pin();
    else unseen += 1;
    drawLatest();
  };

  const replace = (entries: ConversationEntry[]): void => {
    thinkingLine = null;
    let before: string | null = null;
    const drawn = entries.map((entry) => {
      const line = draw(entry, before);
      before = entry.role;
      return line;
    });
    lines.replaceChildren(...drawn);
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
    if (selectedAttachments.length >= 8 || attach.disabled) return;
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
    say(backlog.notice);
  });

  void listen("scufris://notice", (event) => {
    say(event.payload as Notice);
  });

  // The window is built at startup and filled whether it is on screen or not,
  // so there is usually a backlog by the time anybody opens it.
  void (async () => {
    const backlog = (await invoke("hud_ready")) as Backlog;
    replace(backlog.lines);
    say(backlog.notice);
    fit();
    words.focus();
  })();
}
