// The page every widget window loads, from the widgets design page section 02
// (tasks/20260825-194822/scufris-widgets.html). It owns the chrome and the
// tokens; the widget owns #root and nothing else.
//
// It draws nothing on its own clock. WebKitGTK throttles hidden pages, and a
// pooled shell is hidden by definition, so everything here happens because a
// message arrived: the host sends, the page renders. Nothing polls, nothing
// ticks, nothing waits on rAF.
//
// Wrapped in a block for the same reason review.ts is: it is a classic script,
// so a name at its top level is a name in every other page's global scope.
//
// Compiled by tsc from build.rs into ui/dist; the window loads the output.

{
  const { invoke, Channel } = window.__TAURI__.core;

  // How long the badge carries a refused tick's reason. Long enough to read a
  // few words, short enough that the panel is not left wearing an error.
  const REFUSAL_MS = 2400;

  const forward = (level: string, message: string): void => {
    try {
      invoke("pill_log", { level: `widget.${level}`, message }).catch(() => {});
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
    if (found === null) throw new Error(`the shell page is missing #${id}`);
    return found as T;
  };

  const panel = element<HTMLElement>("panel");
  const title = element<HTMLElement>("title");
  const badge = element<HTMLElement>("badge");
  const root = element<HTMLElement>("root");
  const close = element<HTMLButtonElement>("close");
  const pin = element<HTMLButtonElement>("pin");
  const again = element<HTMLButtonElement>("again");

  // The widget the window is currently holding, and the data that arrived
  // before it finished loading. A module is imported once and takes a moment;
  // an update in that moment is the widget's first data, not a lost message.
  let view: WidgetView | null = null;
  let pending: unknown = undefined;
  let waiting = false;

  const broken = (reason: string): void => {
    forward("error", reason);
    root.replaceChildren();
    const said = document.createElement("p");
    said.className = "broken";
    said.textContent = reason;
    root.append(said);
  };

  const deliver = (data: unknown): void => {
    if (view === null) {
      // Held rather than dropped. The module is still importing, and this is
      // the payload it was opened to show.
      pending = data;
      return;
    }
    try {
      view.update(data);
    } catch (error) {
      broken(`${String(error)}`);
    }
  };

  // Served by the scufris-widget: scheme, which answers only with the widget
  // the requesting window is currently holding. The URL says nothing the host
  // does not already know, which is why there is only one of it. Typed as a
  // plain string so the compiler treats it as an address rather than as a
  // module it has to find on disk.
  const module: string = "scufris-widget://localhost/widget.js";

  const become = (message: Extract<ShellMsg, { kind: "become" }>): void => {
    if (waiting) return;
    waiting = true;
    title.textContent = message.name;
    import(module)
      .then((loaded: unknown) => {
        const mount = (loaded as Partial<WidgetModule>).mount;
        if (typeof mount !== "function") {
          broken(`${message.widget} exports no mount`);
          return;
        }
        const ctx: WidgetContext = {
          spawn: message.data,
          send: (action: unknown): void => {
            // One line onto the backend's input, the mirror of the lines it
            // writes. The host names the surface from the window, so nothing
            // here says which panel this is. A widget with no backend behind
            // it is refused on the badge rather than left believing it landed.
            invoke("widget_send", { action }).catch(() => {});
          },
        };
        view = mount(root, ctx);
        deliver(pending === undefined ? message.data : pending);
        pending = undefined;
      })
      .catch((error: unknown) => {
        broken(`${message.widget} would not load: ${String(error)}`);
      });
  };

  const retire = (): void => {
    if (view === null) return;
    try {
      view.destroy();
    } catch (error) {
      forward("error", `destroy: ${String(error)}`);
    }
    view = null;
    root.replaceChildren();
  };

  // The two things the panel can say about itself, and the refusal that
  // borrows the badge from both for a moment.
  let life = "live";
  let health = "fresh";
  let refusing: ReturnType<typeof setTimeout> | undefined;

  // A backend that stopped is the more urgent of the two, so it takes the
  // badge while it holds. A panel whose sampler is dead saying "dim" would be
  // answering a question nobody asked.
  const said = (): string => (health === "fresh" ? life : health);

  const say = (): void => {
    if (refusing === undefined) badge.textContent = said();
  };

  // A tick that silently does nothing reads as a tick that is broken. The
  // badge carries the reason for a moment and then goes back to saying what
  // the panel is.
  const refuse = (detail: string): void => {
    clearTimeout(refusing);
    badge.textContent = detail;
    panel.dataset["refused"] = "";
    refusing = setTimeout(() => {
      refusing = undefined;
      badge.textContent = said();
      delete panel.dataset["refused"];
    }, REFUSAL_MS);
  };

  const handle = (message: ShellMsg): void => {
    switch (message.kind) {
      case "become":
        become(message);
        break;
      case "update":
        deliver(message.data);
        break;
      case "life":
        life = message.state;
        panel.dataset["life"] = life;
        say();
        break;
      case "health":
        health = message.state;
        // Absent rather than "fresh", so every rule about a backend that is
        // in trouble is a rule that only matches when one is.
        if (health === "fresh") delete panel.dataset["health"];
        else panel.dataset["health"] = health;
        say();
        break;
      case "refused":
        refuse(message.detail);
        break;
      case "retire":
        retire();
        break;
    }
  };

  // The two chrome ticks. Clicks, not keys: the window is built unfocusable so
  // that a widget landing mid-sentence never takes the keyboard, and a click
  // has never needed focus.
  close.addEventListener("click", () => {
    invoke("widget_tick", { kind: "close" }).catch(() => {});
  });

  pin.addEventListener("click", () => {
    invoke("widget_tick", { kind: "pin" }).catch(() => {});
  });

  // Only reachable while the backend behind this panel is gone, which is when
  // starting it again is the one thing worth offering.
  again.addEventListener("click", () => {
    invoke("widget_tick", { kind: "restart" }).catch(() => {});
  });

  // A panel somebody is reading does not age out from under them. The pointer
  // is the only thing that says so - the window never takes the keyboard - and
  // only this page can see it, so it is this page that reports it.
  const hover = (over: boolean) => (): void => {
    invoke("widget_hover", { over }).catch(() => {});
  };

  panel.addEventListener("mouseenter", hover(true));
  panel.addEventListener("mouseleave", hover(false));

  const channel = new Channel<ShellMsg>();
  channel.onmessage = handle;
  // Last, and only once the page can act on what comes back: the host treats
  // this as the shell being ready and may send `become` immediately.
  invoke("widget_shell_ready", { channel }).catch((error: unknown) => {
    forward("error", `the shell could not report itself: ${String(error)}`);
  });
}
