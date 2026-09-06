// Safe Markdown rendering for the conversation page.
//
// This renderer never turns response text into HTML. It creates a small set of
// semantic DOM elements and puts all response content into text nodes. Only
// credential-free HTTP and HTTPS URLs with a host become links. The Rust command
// that opens a link validates the same rule again.

{
  type OpenLink = (url: string) => void;

  interface ListMarker {
    ordered: boolean;
    start: number;
    content: string;
  }

  const safeUrl = (raw: string): string | null => {
    if (
      raw.length > 8 * 1024 ||
      /[\u0000-\u001f\u007f]/u.test(raw) ||
      !/^https?:\/\/[^/]/iu.test(raw)
    )
      return null;
    try {
      const parsed = new URL(raw);
      if (
        (parsed.protocol !== "http:" && parsed.protocol !== "https:") ||
        parsed.hostname === "" ||
        parsed.username !== "" ||
        parsed.password !== ""
      )
        return null;
      return parsed.href;
    } catch {
      return null;
    }
  };

  const text = (parent: HTMLElement, value: string): void => {
    if (value !== "") parent.append(document.createTextNode(value));
  };

  const link = (
    parent: HTMLElement,
    label: string,
    rawUrl: string,
    open: OpenLink,
  ): boolean => {
    const url = safeUrl(rawUrl);
    if (url === null) return false;
    const anchor = document.createElement("a");
    anchor.setAttribute("href", url);
    anchor.setAttribute("rel", "noreferrer");
    anchor.textContent = label;
    anchor.addEventListener("click", (event) => {
      event.preventDefault();
      open(url);
    });
    parent.append(anchor);
    return true;
  };

  /** Removes punctuation that belongs to the prose after one bare URL. */
  const bareUrlLength = (candidate: string): number => {
    let length = candidate.length;
    while (length > 0 && /[.,!?;:]/u.test(candidate[length - 1] ?? "")) {
      length -= 1;
    }
    for (const [opening, closing] of [
      ["(", ")"],
      ["[", "]"],
      ["{", "}"],
    ]) {
      while (
        length > 0 &&
        candidate[length - 1] === closing &&
        [...candidate.slice(0, length)].filter((value) => value === closing)
          .length >
          [...candidate.slice(0, length)].filter((value) => value === opening)
            .length
      ) {
        length -= 1;
      }
    }
    return length;
  };

  const bareUrlAt = (source: string, index: number): string | null => {
    const before = source[index - 1];
    if (before !== undefined && /[A-Za-z0-9+.:_-]/u.test(before)) return null;
    const found = /^(?:https?):\/\/[^\s<>"'`]+/iu.exec(source.slice(index));
    if (found === null) return null;
    const candidate = found[0];
    const length = bareUrlLength(candidate);
    return length === 0 ? null : candidate.slice(0, length);
  };

  const closing = (source: string, marker: string, after: number): number => {
    let found = source.indexOf(marker, after);
    while (found >= 0) {
      let escapes = 0;
      for (
        let index = found - 1;
        index >= 0 && source[index] === "\\";
        index -= 1
      )
        escapes += 1;
      if (escapes % 2 === 0) return found;
      found = source.indexOf(marker, found + marker.length);
    }
    return -1;
  };

  const markdownDestination = (
    source: string,
    opening: number,
  ): { end: number; url: string } | null => {
    let depth = 0;
    let escaped = false;
    for (let index = opening + 1; index < source.length; index += 1) {
      const character = source[index] ?? "";
      if (escaped) {
        escaped = false;
        continue;
      }
      if (character === "\\") {
        escaped = true;
        continue;
      }
      if (character === "(") {
        depth += 1;
        continue;
      }
      if (character !== ")") continue;
      if (depth > 0) {
        depth -= 1;
        continue;
      }
      let destination = source.slice(opening + 1, index).trim();
      if (destination.startsWith("<") && destination.endsWith(">")) {
        destination = destination.slice(1, -1);
      } else {
        // A quoted Markdown title is not part of the URL. URLs containing
        // spaces must use angle brackets, which keeps this split unambiguous.
        destination = destination.split(/[ \t]+/u, 1)[0] ?? "";
      }
      return { end: index + 1, url: destination };
    }
    return null;
  };

  const renderInline = (
    parent: HTMLElement,
    source: string,
    open: OpenLink,
    depth = 0,
  ): void => {
    // Bound recursion from nested emphasis or link labels independently of the
    // already bounded protocol field.
    if (depth > 32) {
      text(parent, source);
      return;
    }

    let literal = "";
    const flush = (): void => {
      text(parent, literal);
      literal = "";
    };

    for (let index = 0; index < source.length; ) {
      const character = source[index] ?? "";

      if (character === "\\" && index + 1 < source.length) {
        const escaped = source[index + 1] ?? "";
        if (/^[!"#$%&'()*+,\-./:;<=>?@[\\\]^_`{|}~]$/u.test(escaped)) {
          literal += escaped;
          index += 2;
          continue;
        }
      }

      if (character === "`") {
        let width = 1;
        while (source[index + width] === "`") width += 1;
        const marker = "`".repeat(width);
        const end = closing(source, marker, index + width);
        if (end >= 0) {
          flush();
          const code = document.createElement("code");
          code.className = "inline-code";
          code.textContent = source
            .slice(index + width, end)
            .replaceAll("\n", " ")
            .replace(/^ | $/gu, "");
          parent.append(code);
          index = end + width;
          continue;
        }
      }

      if (character === "[") {
        const labelEnd = closing(source, "]", index + 1);
        if (labelEnd >= 0 && source[labelEnd + 1] === "(") {
          const destination = markdownDestination(source, labelEnd + 1);
          if (destination !== null) {
            flush();
            const label = source.slice(index + 1, labelEnd);
            const url = safeUrl(destination.url);
            if (url === null) {
              renderInline(parent, label, open, depth + 1);
            } else {
              const anchor = document.createElement("a");
              anchor.setAttribute("href", url);
              anchor.setAttribute("rel", "noreferrer");
              renderInline(anchor, label, open, depth + 1);
              anchor.addEventListener("click", (event) => {
                event.preventDefault();
                open(url);
              });
              parent.append(anchor);
            }
            index = destination.end;
            continue;
          }
        }
      }

      const strongMarker = source.startsWith("**", index)
        ? "**"
        : source.startsWith("__", index)
          ? "__"
          : null;
      if (strongMarker !== null) {
        const end = closing(source, strongMarker, index + 2);
        if (end > index + 2) {
          flush();
          const strong = document.createElement("strong");
          renderInline(strong, source.slice(index + 2, end), open, depth + 1);
          parent.append(strong);
          index = end + 2;
          continue;
        }
      }

      if (character === "*" || character === "_") {
        const end = closing(source, character, index + 1);
        if (end > index + 1) {
          flush();
          const emphasis = document.createElement("em");
          renderInline(emphasis, source.slice(index + 1, end), open, depth + 1);
          parent.append(emphasis);
          index = end + 1;
          continue;
        }
      }

      if (character === "<") {
        const end = source.indexOf(">", index + 1);
        if (end >= 0) {
          const destination = source.slice(index + 1, end);
          const url = safeUrl(destination);
          if (url !== null) {
            flush();
            link(parent, destination, url, open);
            index = end + 1;
            continue;
          }
        }
      }

      const lastAngleOpen = source.lastIndexOf("<", index);
      const lastAngleClose = source.lastIndexOf(">", index);
      const insideRawHtml =
        lastAngleOpen > lastAngleClose && source.indexOf(">", index) >= 0;
      const bare = insideRawHtml ? null : bareUrlAt(source, index);
      if (bare !== null) {
        const url = safeUrl(bare);
        if (url !== null) {
          flush();
          link(parent, bare, url, open);
          index += bare.length;
          continue;
        }
      }

      if (character === "\n") {
        if (literal.endsWith("  ")) {
          literal = literal.slice(0, -2);
          flush();
          parent.append(document.createElement("br"));
        } else {
          literal += " ";
        }
        index += 1;
        continue;
      }

      literal += character;
      index += 1;
    }
    flush();
  };

  const fence = (
    line: string,
  ): { character: string; width: number; language: string } | null => {
    const found = /^ {0,3}(`{3,}|~{3,})[ \t]*([^ \t`]*)[ \t]*$/u.exec(line);
    if (found === null) return null;
    const marker = found[1] ?? "";
    return {
      character: marker[0] ?? "`",
      width: marker.length,
      language: found[2] ?? "",
    };
  };

  const heading = (line: string): { level: number; content: string } | null => {
    const found = /^ {0,3}(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$/u.exec(line);
    if (found === null) return null;
    return { level: (found[1] ?? "#").length, content: found[2] ?? "" };
  };

  const thematic = (line: string): boolean =>
    /^ {0,3}(?:(?:\*[ \t]*){3,}|(?:-[ \t]*){3,}|(?:_[ \t]*){3,})$/u.test(line);

  const quote = (line: string): string | null => {
    const found = /^ {0,3}>[ \t]?(.*)$/u.exec(line);
    return found === null ? null : (found[1] ?? "");
  };

  const listMarker = (line: string): ListMarker | null => {
    const unordered = /^ {0,3}[-+*][ \t]+(.*)$/u.exec(line);
    if (unordered !== null)
      return { ordered: false, start: 1, content: unordered[1] ?? "" };
    const ordered = /^ {0,3}(\d{1,9})[.)][ \t]+(.*)$/u.exec(line);
    if (ordered === null) return null;
    return {
      ordered: true,
      start: Number(ordered[1] ?? "1"),
      content: ordered[2] ?? "",
    };
  };

  const startsBlock = (line: string): boolean =>
    fence(line) !== null ||
    heading(line) !== null ||
    thematic(line) ||
    quote(line) !== null ||
    listMarker(line) !== null;

  const renderBlocks = (
    parent: HTMLElement,
    source: string,
    open: OpenLink,
  ): void => {
    const lines = source
      .replaceAll("\r\n", "\n")
      .replaceAll("\r", "\n")
      .split("\n");
    let index = 0;
    while (index < lines.length) {
      const line = lines[index] ?? "";
      if (line.trim() === "") {
        index += 1;
        continue;
      }

      const openingFence = fence(line);
      if (openingFence !== null) {
        index += 1;
        const body: string[] = [];
        const close = new RegExp(
          `^ {0,3}${openingFence.character}{${String(openingFence.width)},}[ \\t]*$`,
          "u",
        );
        while (index < lines.length && !close.test(lines[index] ?? "")) {
          body.push(lines[index] ?? "");
          index += 1;
        }
        if (index < lines.length) index += 1;
        const pre = document.createElement("pre");
        pre.className = "code-block";
        const code = document.createElement("code");
        code.textContent = body.join("\n");
        if (openingFence.language !== "") {
          code.dataset["language"] = openingFence.language;
          code.setAttribute("aria-label", `${openingFence.language} code`);
        }
        pre.append(code);
        parent.append(pre);
        continue;
      }

      const oneHeading = heading(line);
      if (oneHeading !== null) {
        const title = document.createElement(`h${String(oneHeading.level)}`);
        renderInline(title, oneHeading.content, open);
        parent.append(title);
        index += 1;
        continue;
      }

      if (thematic(line)) {
        parent.append(document.createElement("hr"));
        index += 1;
        continue;
      }

      if (quote(line) !== null) {
        const quoted: string[] = [];
        while (index < lines.length) {
          const content = quote(lines[index] ?? "");
          if (content === null) break;
          quoted.push(content);
          index += 1;
        }
        const block = document.createElement("blockquote");
        renderBlocks(block, quoted.join("\n"), open);
        parent.append(block);
        continue;
      }

      const firstMarker = listMarker(line);
      if (firstMarker !== null) {
        const list = document.createElement(firstMarker.ordered ? "ol" : "ul");
        if (firstMarker.ordered && firstMarker.start !== 1)
          list.setAttribute("start", String(firstMarker.start));
        while (index < lines.length) {
          const marker = listMarker(lines[index] ?? "");
          if (marker === null || marker.ordered !== firstMarker.ordered) break;
          const item = document.createElement("li");
          renderInline(item, marker.content, open);
          list.append(item);
          index += 1;
        }
        parent.append(list);
        continue;
      }

      const paragraph: string[] = [line.trim()];
      index += 1;
      while (index < lines.length) {
        const next = lines[index] ?? "";
        if (next.trim() === "" || startsBlock(next)) break;
        paragraph.push(next.trim());
        index += 1;
      }
      const body = document.createElement("p");
      renderInline(body, paragraph.join("\n"), open);
      parent.append(body);
    }
  };

  window.scufrisMarkup = {
    renderPlain(parent: HTMLElement, source: string, open: OpenLink): void {
      // Keep the common path as one literal text node. In particular, Markdown
      // punctuation in required response text is never interpreted.
      if (!/(?:https?):\/\//iu.test(source)) {
        parent.textContent = source;
        return;
      }
      parent.replaceChildren();
      let literal = "";
      const flush = (): void => {
        text(parent, literal);
        literal = "";
      };
      for (let index = 0; index < source.length; ) {
        const bare = bareUrlAt(source, index);
        if (bare === null) {
          literal += source[index] ?? "";
          index += 1;
          continue;
        }
        const url = safeUrl(bare);
        if (url === null) {
          literal += bare;
          index += bare.length;
          continue;
        }
        flush();
        link(parent, bare, url, open);
        index += bare.length;
      }
      flush();
    },

    renderDetails(parent: HTMLElement, source: string, open: OpenLink): void {
      parent.replaceChildren();
      renderBlocks(parent, source, open);
    },
  };
}
