// The whole widget contract, and the only copy of it.
//
// Shared by every widget in this directory and by the shell that mounts them:
// `shell/tsconfig.json` reads this same file. Two copies of a contract are two
// contracts, and the day they drift is a day both sides compile and the panel
// breaks in front of the person.
//
// A widget renders into the element the shell hands it and repaints when new
// data arrives. It never draws chrome, never asks who sent the data, and never
// runs on a clock of its own: the shell owns the frame, and WebKitGTK throttles
// a hidden page, so anything that ticks in here stops when the window is put
// away and lies about the time when it comes back.

/** One field on a form a widget asked for. */
interface WidgetField {
  /** The key the answer arrives under, beside the action's own keys. */
  name: string;
  /** What is printed over the field. */
  label: string;
  /** What the field starts with. */
  value?: string;
  /** How many lines the field is. One is a line; more is a block. */
  lines?: number;
  /** Grey words in an empty field. */
  hint?: string;
}

/** A question a widget cannot ask on its own page. */
interface WidgetAsk {
  /** What the box is titled. */
  title: string;
  /** The fields, in the order they are asked. At least one, at most four. */
  fields: WidgetField[];
  /** The action the answers are laid into, and sent as. Must be an object. */
  action: Record<string, unknown>;
}

/** What the shell hands a widget when it mounts it. */
interface WidgetContext {
  /**
   * The spawn payload the open carried.
   *
   * The widget draws its own first frame from this, inside `mount`. It is not
   * handed to `update` afterwards: for some widgets the spawn payload is the
   * data, and for others it is only the request the data answers.
   */
  spawn: unknown;
  /** Sends one action back toward whatever is feeding this widget. */
  send(action: unknown): void;
  /**
   * Asks the person for words, and sends the answer as one action.
   *
   * A widget window never holds the keyboard - it is built unfocusable so a
   * panel arriving mid-sentence cannot take the keys of whoever was typing -
   * so a field on this page would be one nobody could type in. The words are
   * taken in a small window of the companion's own instead, and what comes
   * back is `action` with one key per field laid into it. So a widget writes
   * by asking, and reads its own writing back through its backend the way it
   * reads everything else.
   *
   * Nothing arrives if the person cancels. The answer is not a return value:
   * it lands as an ordinary action on the backend, one line like any other.
   */
  ask(request: WidgetAsk): void;
}

/** What a widget hands back, so the shell can drive and unmount it. */
interface WidgetView {
  /** Renders new data. Never called with the spawn payload. */
  update(data: unknown): void;
  /** Releases anything the widget holds. The window closes right after. */
  destroy(): void;
}

/** The one export every `widget.ts` has. */
interface WidgetModule {
  mount(root: HTMLElement, ctx: WidgetContext): WidgetView;
}
