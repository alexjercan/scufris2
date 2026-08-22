#!/usr/bin/env python3
"""Serve the bounded Scufris walkthrough review UI over a JSON-lines bridge."""

from __future__ import annotations

import argparse
import html
import json
import re
import secrets
import sys
import threading
import webbrowser
from collections.abc import Callable
from dataclasses import dataclass
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, BinaryIO, TextIO
from urllib.parse import urlparse

MAX_LINE_BYTES = 512 * 1024
MAX_INIT_LINE_BYTES = 4 * 1024 * 1024
MAX_RESULT_LINE_BYTES = 32 * 1024 * 1024
MAX_ACTION_BYTES = 16 * 1024
MAX_COMMENT_BYTES = 4096
MAX_CONTEXT_BYTES = 256 * 1024
MAX_SECTIONS = 40

CSS = r"""
:root{color-scheme:light dark;--bg:#f6f8fa;--panel:#fff;--text:#1f2328;--muted:#656d76;--border:#d0d7de;--blue:#0969da;--green:#1a7f37;--red:#cf222e;--purple:#8250df;--orange:#bc4c00;--add:#dafbe1;--del:#ffebe9;--hunk:#ddf4ff;--code:#f6f8fa;--shadow:0 1px 2px rgba(31,35,40,.08)}
*{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:14px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}main{max-width:1120px;margin:auto;padding:40px 24px 80px}.hero,.card,.final{background:var(--panel);border:1px solid var(--border);border-radius:12px;box-shadow:var(--shadow)}.hero{padding:28px 32px;margin-bottom:22px}.eyebrow{color:var(--muted);font-weight:600;margin:0 0 5px}.hero h1{font-size:30px;line-height:1.25;margin:0 0 16px}.chips,.meta,.actions{display:flex;flex-wrap:wrap;gap:8px}.chip,.badge{display:inline-flex;align-items:center;border:1px solid var(--border);border-radius:999px;padding:3px 9px;font-size:12px;font-weight:600;background:var(--bg)}.chip.good{color:var(--green);background:#dafbe1;border-color:#aceebb}.chip.add{color:var(--green)}.chip.del{color:var(--red)}.built{border-top:1px solid var(--border);margin-top:20px;padding-top:18px}.built h2{font-size:16px;margin:0 0 7px}.built p,.prose p{margin:6px 0}.progress-wrap{margin:20px 0}.progress-label{display:flex;justify-content:space-between;color:var(--muted);margin-bottom:7px}.progress{appearance:none;width:100%;height:8px;border:0;border-radius:99px;background:#d8dee4;overflow:hidden}.progress::-webkit-progress-bar{background:#d8dee4}.progress::-webkit-progress-value{background:var(--green)}.progress::-moz-progress-bar{background:var(--green)}.card{margin:16px 0;overflow:hidden}.card-head{padding:20px 24px 14px;display:flex;gap:16px;justify-content:space-between;align-items:flex-start}.card h2{font-size:19px;margin:0 0 7px}.number{color:var(--muted);font-weight:500}.meta{color:var(--muted);font-size:13px}.meta code{background:var(--code);border-radius:5px;padding:2px 6px}.importance-critical{color:var(--red);background:#ffebe9}.importance-important{color:var(--orange);background:#fff8c5}.importance-supporting{color:var(--purple);background:#fbefff}.state-not-reviewed{color:var(--muted)}.state-looks-good{color:var(--green);background:#dafbe1}.state-needs-explanation{color:var(--blue);background:#ddf4ff}.state-change-requested{color:var(--red);background:#ffebe9}.prose{padding:0 24px 16px;font-size:15px}.diff{margin:0;border-top:1px solid var(--border);border-bottom:1px solid var(--border);overflow:auto;background:var(--code);font:12px/20px ui-monospace,SFMono-Regular,Consolas,monospace}.diff-line{display:block;white-space:pre;min-width:max-content;padding:0 16px;border-left:3px solid transparent}.diff-add{background:var(--add);border-color:var(--green)}.diff-del{background:var(--del);border-color:var(--red)}.diff-hunk{background:var(--hunk);color:var(--blue)}.diff-file{font-weight:600;color:var(--muted)}.prompt{margin:18px 24px;padding:12px 14px;border-left:4px solid var(--blue);background:#ddf4ff;border-radius:0 6px 6px 0}.prompt strong{display:block;color:var(--blue);font-size:12px;text-transform:uppercase;letter-spacing:.04em}.answers{padding:0 24px}.answer{border:1px solid var(--border);border-radius:8px;padding:12px 14px;margin:10px 0;background:var(--bg)}.answer strong{display:block;margin-bottom:4px}.controls{padding:16px 24px 22px}.comment{width:100%;border:1px solid var(--border);border-radius:6px;background:var(--panel);color:var(--text);padding:9px 11px;margin:0 0 10px;font:inherit}.button{appearance:none;border:1px solid rgba(31,35,40,.15);border-radius:6px;background:#f6f8fa;color:#24292f;font-weight:600;padding:7px 13px;cursor:pointer;box-shadow:0 1px 0 rgba(31,35,40,.04)}.button:hover{background:#f3f4f6}.button:disabled{opacity:.55;cursor:wait}.button.primary{color:#fff;background:#1f883d}.button.danger{color:#fff;background:#cf222e}.button.link{color:var(--blue);background:var(--panel)}.feedback{min-height:22px;margin-top:8px;color:var(--muted)}.feedback.error{color:var(--red)}.feedback.success{color:var(--green)}.context-view{max-height:420px;overflow:auto;margin:10px 0 0;padding:12px 14px;border:1px solid var(--border);border-radius:6px;background:var(--code);color:var(--text);font:12px/20px ui-monospace,SFMono-Regular,Consolas,monospace;white-space:pre}.final{padding:24px 28px;margin-top:24px}.final h2{margin:0 0 4px}.final .actions{margin-top:16px}.warnings{color:var(--orange);margin:12px 0}.revision{font-family:ui-monospace,monospace}.spinner{display:inline-block;width:12px;height:12px;border:2px solid currentColor;border-right-color:transparent;border-radius:50%;animation:spin .7s linear infinite;margin-right:6px;vertical-align:-2px}@keyframes spin{to{transform:rotate(360deg)}}
@media(max-width:650px){main{padding:18px 10px 50px}.hero{padding:21px}.card-head{padding:17px;display:block}.card-head>.badge{margin-top:10px}.prose,.controls,.answers{padding-left:17px;padding-right:17px}.prompt{margin-left:17px;margin-right:17px}.actions .button{flex:1}}
@media(prefers-color-scheme:dark){:root{--bg:#0d1117;--panel:#161b22;--text:#e6edf3;--muted:#8b949e;--border:#30363d;--blue:#58a6ff;--green:#3fb950;--red:#f85149;--purple:#bc8cff;--orange:#d29922;--add:#12261e;--del:#2d1619;--hunk:#13283a;--code:#0d1117}.button{background:#21262d;color:#e6edf3;border-color:#30363d}.chip.good,.state-looks-good{background:#12261e;border-color:#238636}.importance-critical,.state-change-requested{background:#2d1619}.importance-important{background:#302b13}.importance-supporting{background:#21183c}.state-needs-explanation,.prompt{background:#13283a}}
"""

JS = r"""
const root=document.querySelector('main');
let currentState=null;
const stateLabels={'not-reviewed':'Not reviewed','looks-good':'Looks good','needs-explanation':'Needs explanation','change-requested':'Change requested'};
function escapeText(value){return document.createTextNode(value)}
function renderState(state){
  currentState=state;
  let reviewed=0;
  for(const [id,value] of Object.entries(state.sections)){
    if(value==='looks-good')reviewed++;
    const badge=document.querySelector(`[data-state="${CSS.escape(id)}"]`);
    if(badge){badge.className=`badge state-${value}`;badge.replaceChildren(escapeText(stateLabels[value]||value));}
    const answers=document.querySelector(`[data-answers="${CSS.escape(id)}"]`);
    if(answers){answers.replaceChildren();for(const item of state.questions.filter(q=>q.sectionId===id&&q.answer)){
      const box=document.createElement('div');box.className='answer';const strong=document.createElement('strong');strong.textContent=item.question;const text=document.createElement('div');text.textContent=item.answer;box.append(strong,text);answers.append(box);
    }}
  }
  const total=Object.keys(state.sections).length;document.querySelectorAll('[data-reviewed]').forEach(x=>x.textContent=reviewed);document.querySelectorAll('[data-total]').forEach(x=>x.textContent=total);document.querySelectorAll('[data-progress]').forEach(x=>{x.max=total;x.value=reviewed});
  const approve=document.querySelector('[data-action="approve"]');if(approve)approve.disabled=reviewed!==total||state.approved;
}
async function act(button){
  const scope=button.closest('[data-scope]');const feedback=scope.querySelector('.feedback');const buttons=scope.querySelectorAll('button');buttons.forEach(x=>x.disabled=true);feedback.className='feedback';feedback.innerHTML='<span class="spinner"></span>Working...';
  const comment=scope.querySelector('.comment');const payload={action:button.dataset.action};if(scope.dataset.section)payload.section=scope.dataset.section;if(comment)payload.comment=comment.value;
  try{const response=await fetch('action',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(payload)});let result;try{result=await response.json()}catch{throw new Error(`Request failed (${response.status})`)}if(result.state)renderState(result.state);if(!response.ok||!result.ok)throw new Error(result.error||`Request failed (${response.status})`);if(typeof result.context==='string'){const view=scope.querySelector('.context-view');if(view){view.querySelector('code').textContent=result.context;view.hidden=false;}}feedback.textContent=result.message||'Updated.';feedback.className='feedback success';if(comment&&['ask','request-change'].includes(payload.action))comment.value='';}
  catch(error){feedback.textContent=error instanceof Error?error.message:String(error);feedback.className='feedback error';}
  finally{buttons.forEach(x=>x.disabled=false);if(currentState)renderState(currentState);}
}
root.addEventListener('click',event=>{const button=event.target.closest('button[data-action]');if(button){event.preventDefault();act(button)}});
"""


def _text(value: Any, maximum: int = 262_144) -> str:
    if not isinstance(value, str) or len(value.encode("utf-8")) > maximum:
        raise ValueError("descriptor contains invalid text")
    return value


def validate_init(message: Any) -> dict[str, Any]:
    if not isinstance(message, dict) or set(message) != {
        "type",
        "version",
        "document",
        "state",
    }:
        raise ValueError("initial bridge message is invalid")
    if message["type"] != "init" or message["version"] != 1:
        raise ValueError("unsupported bridge protocol")
    document = message["document"]
    state = message["state"]
    if not isinstance(document, dict) or not isinstance(state, dict):
        raise TypeError("descriptor is invalid")
    sections = document.get("sections")
    if not isinstance(sections, list) or not 1 <= len(sections) <= MAX_SECTIONS:
        raise ValueError("descriptor sections are invalid")
    required_document = {
        "title",
        "summary",
        "revision",
        "baseRevision",
        "files",
        "added",
        "removed",
        "preflight",
        "sections",
        "warnings",
    }
    if set(document) != required_document:
        raise ValueError("document descriptor schema is invalid")
    _text(document["title"], 400)
    _text(document["summary"])
    _text(document["revision"], 80)
    _text(document["baseRevision"], 80)
    if document["preflight"] != "passed" or not all(
        isinstance(document[key], int) and document[key] >= 0
        for key in ("files", "added", "removed")
    ):
        raise ValueError("document metadata is invalid")
    ids: set[str] = set()
    for section in sections:
        required = {"id", "importance", "file", "lines", "markdown", "diff", "prompt"}
        if not isinstance(section, dict) or set(section) != required:
            raise ValueError("section descriptor schema is invalid")
        for key in required:
            _text(section[key], 262_144 if key in {"markdown", "diff"} else 4096)
        if (
            section["importance"] not in {"critical", "important", "supporting"}
            or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", section["id"])
            or section["id"] in ids
        ):
            raise ValueError("section descriptor values are invalid")
        ids.add(section["id"])
    if not isinstance(document["warnings"], list) or any(
        not isinstance(item, str) for item in document["warnings"]
    ):
        raise ValueError("warnings are invalid")
    required_state = {
        "version",
        "identity",
        "revision",
        "sections",
        "questions",
        "changeRequests",
        "approved",
    }
    section_states = state.get("sections")
    if (
        set(state) != required_state
        or state["version"] != 1
        or not isinstance(state["identity"], str)
        or state["revision"] != document["revision"]
        or not isinstance(section_states, dict)
        or set(section_states) != ids
        or any(
            value
            not in {
                "not-reviewed",
                "looks-good",
                "needs-explanation",
                "change-requested",
            }
            for value in section_states.values()
        )
        or not isinstance(state["questions"], list)
        or not isinstance(state["changeRequests"], list)
        or not isinstance(state["approved"], bool)
    ):
        raise ValueError("state does not match document")
    for question in state["questions"]:
        if (
            not isinstance(question, dict)
            or not {"sectionId", "question"}.issubset(question)
            or not set(question).issubset({"sectionId", "question", "answer"})
            or question["sectionId"] not in ids
            or not isinstance(question["question"], str)
            or ("answer" in question and not isinstance(question["answer"], str))
        ):
            raise ValueError("state questions are invalid")
    for request in state["changeRequests"]:
        if (
            not isinstance(request, dict)
            or set(request) != {"sectionId", "feedback"}
            or request["sectionId"] not in ids
            or not isinstance(request["feedback"], str)
        ):
            raise ValueError("state change requests are invalid")
    if state["approved"] and any(
        value != "looks-good" for value in section_states.values()
    ):
        raise ValueError("approved state has unresolved sections")
    return message


def validate_result(result: Any, document: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(result, dict) or not isinstance(result.get("ok"), bool):
        raise TypeError("bridge result schema is invalid")
    expected = (
        {"type", "id", "ok", "state", "message"}
        if result["ok"]
        else {"type", "id", "ok", "state", "error"}
    )
    if result["ok"] and "context" in result:
        expected.add("context")
    text_key = "message" if result["ok"] else "error"
    if (
        set(result) != expected
        or result.get("type") != "result"
        or not isinstance(result.get("id"), str)
        or not isinstance(result.get(text_key), str)
        or len(result[text_key].encode("utf-8")) > MAX_LINE_BYTES
        or (
            "context" in result
            and (
                not isinstance(result["context"], str)
                or len(result["context"].encode("utf-8")) > MAX_CONTEXT_BYTES
            )
        )
    ):
        raise ValueError("bridge result schema is invalid")
    validate_init(
        {"type": "init", "version": 1, "document": document, "state": result["state"]}
    )
    return result


def read_json_line(stream: BinaryIO, maximum: int = MAX_LINE_BYTES) -> Any:
    line = stream.readline(maximum + 1)
    if not line:
        raise EOFError("bridge closed")
    if len(line) > maximum or not line.endswith(b"\n"):
        raise ValueError("bridge message exceeds limit")
    try:
        return json.loads(line)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("bridge message is malformed") from error


def prose(value: str) -> str:
    return "".join(
        f"<p>{html.escape(part).replace(chr(10), '<br>')}</p>"
        for part in value.split("\n\n")
        if part.strip()
    )


def diff_html(value: str) -> str:
    lines = []
    for line in value.splitlines():
        kind = "context"
        if line.startswith("@@"):
            kind = "hunk"
        elif line.startswith(("diff ", "index ", "---", "+++")):
            kind = "file"
        elif line.startswith("+"):
            kind = "add"
        elif line.startswith("-"):
            kind = "del"
        lines.append(
            f'<span class="diff-line diff-{kind}">{html.escape(line) or " "}</span>'
        )
    return "".join(lines)


def render_page(document: dict[str, Any], state: dict[str, Any]) -> str:
    reviewed = sum(value == "looks-good" for value in state["sections"].values())
    total = len(document["sections"])
    cards = []
    labels = {
        "not-reviewed": "Not reviewed",
        "looks-good": "Looks good",
        "needs-explanation": "Needs explanation",
        "change-requested": "Change requested",
    }
    for index, section in enumerate(document["sections"], 1):
        section_id = html.escape(section["id"], quote=True)
        value = state["sections"][section["id"]]
        answers = "".join(
            f'<div class="answer"><strong>{html.escape(item["question"])}</strong><div>{html.escape(item["answer"])}</div></div>'
            for item in state.get("questions", [])
            if item.get("sectionId") == section["id"] and item.get("answer")
        )
        cards.append(
            f'''<article class="card" id="change-{section_id}"><header class="card-head"><div><h2><span class="number">{index}.</span> {html.escape(section["id"].replace("-", " ").title())}</h2><div class="meta"><span class="badge importance-{section["importance"]}">{html.escape(section["importance"].title())}</span><code>{html.escape(section["file"])}:{html.escape(section["lines"])}</code></div></div><span class="badge state-{value}" data-state="{section_id}">{labels[value]}</span></header><div class="prose">{prose(section["markdown"])}</div><pre class="diff" aria-label="Git diff">{diff_html(section["diff"])}</pre><div class="prompt"><strong>Review prompt</strong>{html.escape(section["prompt"])}</div><div class="answers" data-answers="{section_id}">{answers}</div><div class="controls" data-scope data-section="{section_id}"><input class="comment" maxlength="4096" placeholder="Optional question or change details" aria-label="Question or change details"><div class="actions"><button class="button primary" data-action="looks-good">Looks good</button><button class="button" data-action="explain">Explain</button><button class="button danger" data-action="request-change">Request change</button><button class="button" data-action="context">Show context</button><button class="button link" data-action="ask">Ask reviewer</button></div><div class="feedback" role="status" aria-live="polite"></div><pre class="context-view" aria-label="Exact-revision file context" hidden><code></code></pre></div></article>'''
        )
    warnings = "".join(
        f'<div class="warnings">Warning: {html.escape(item)}</div>'
        for item in document["warnings"]
    )
    return f"""<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><meta name="referrer" content="no-referrer"><title>{html.escape(document["title"])}</title><link rel="stylesheet" href="style.css"></head><body><main><section class="hero"><p class="eyebrow">Scufris implementation walkthrough</p><h1>{html.escape(document["title"])}</h1><div class="chips"><span class="chip good">Preflight passed</span><span class="chip">{document["files"]} files</span><span class="chip add">+{document["added"]}</span><span class="chip del">-{document["removed"]}</span><span class="chip revision">{html.escape(document["revision"][:12])}</span></div><div class="built"><h2>What was built</h2>{prose(document["summary"])}</div>{warnings}</section><div class="progress-wrap"><div class="progress-label"><strong>Review progress</strong><span><span data-reviewed>{reviewed}</span>/<span data-total>{total}</span> sections</span></div><progress class="progress" data-progress value="{reviewed}" max="{total}"></progress></div>{"".join(cards)}<section class="final" data-scope><h2>Final review</h2><p>Approve after every section looks good, return consolidated feedback, or inspect the exact full diff in Plannotator.</p><div class="actions"><button class="button primary" data-action="approve" {"disabled" if reviewed != total else ""}>Approve</button><button class="button danger" data-action="request-changes">Request changes</button><button class="button" data-action="full-diff">View full diff</button></div><div class="feedback" role="status" aria-live="polite"></div></section></main><script src="app.js" defer></script></body></html>"""


@dataclass
class Pending:
    event: threading.Event
    result: dict[str, Any] | None = None


class Bridge:
    def __init__(
        self,
        reader: BinaryIO,
        writer: TextIO,
        opener: Callable[[], Any] = lambda: None,
        shutdown: Callable[[], Any] = lambda: None,
    ) -> None:
        self.reader = reader
        self.writer = writer
        self.opener = opener
        self.shutdown = shutdown
        self.lock = threading.Lock()
        self.pending: dict[str, Pending] = {}
        self.closed = False
        self.activated = False
        self.shutdown_requested = False
        self.thread = threading.Thread(target=self._read_results, daemon=True)

    def start(self) -> None:
        self.thread.start()

    def request(self, action: dict[str, Any]) -> dict[str, Any]:
        request_id = secrets.token_hex(12)
        pending = Pending(threading.Event())
        with self.lock:
            if self.closed:
                raise RuntimeError("review bridge closed")
            self.pending[request_id] = pending
            self.writer.write(
                json.dumps(
                    {"type": "action", "id": request_id, **action},
                    separators=(",", ":"),
                )
                + "\n"
            )
            self.writer.flush()
        pending.event.wait()
        if pending.result is None:
            raise RuntimeError("review bridge closed")
        return pending.result

    def _read_results(self) -> None:
        try:
            while True:
                message = read_json_line(self.reader, MAX_RESULT_LINE_BYTES)
                if message == {"type": "shutdown"} and not self.shutdown_requested:
                    self.shutdown_requested = True
                    self.shutdown()
                    continue
                if message == {"type": "activate"} and not self.activated:
                    self.activated = True
                    try:
                        self.opener()
                    except (OSError, webbrowser.Error):
                        continue
                    continue
                if (
                    not isinstance(message, dict)
                    or message.get("type") != "result"
                    or not isinstance(message.get("id"), str)
                ):
                    raise ValueError("bridge result is invalid")
                with self.lock:
                    pending = self.pending.pop(message["id"], None)
                if pending is None:
                    raise ValueError("bridge result id is unknown")
                pending.result = message
                pending.event.set()
        except (EOFError, ValueError):
            with self.lock:
                self.closed = True
                waiting = list(self.pending.values())
                self.pending.clear()
            for pending in waiting:
                pending.event.set()


class ReviewServer(ThreadingHTTPServer):
    daemon_threads = True
    document: dict[str, Any]
    state: dict[str, Any]
    token: str
    bridge: Bridge
    state_lock: threading.Lock
    action_condition: threading.Condition
    active_actions: int
    shutting_down: bool
    shutdown_thread: threading.Thread | None

    def begin_action(self) -> bool:
        with self.action_condition:
            if self.shutting_down:
                return False
            self.active_actions += 1
            return True

    def end_action(self) -> None:
        with self.action_condition:
            self.active_actions -= 1
            self.action_condition.notify_all()

    def request_shutdown(self) -> None:
        with self.action_condition:
            self.shutting_down = True
            if self.shutdown_thread is not None:
                return
            self.shutdown_thread = threading.Thread(
                target=self._finish_shutdown,
                name="scufris-quick-review-shutdown",
            )
            thread = self.shutdown_thread
        thread.start()

    def _finish_shutdown(self) -> None:
        with self.action_condition:
            while self.active_actions:
                self.action_condition.wait()
        self.shutdown()


class Handler(BaseHTTPRequestHandler):
    server: ReviewServer

    def log_message(self, format: str, *args: object) -> None:
        return

    def _headers(self, status: HTTPStatus, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'none'; font-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'",
        )
        self.end_headers()

    def _send(self, status: HTTPStatus, body: bytes, content_type: str) -> None:
        self._headers(status, content_type)
        self.wfile.write(body)

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        prefix = f"/{self.server.token}/"
        if path == prefix:
            with self.server.state_lock:
                page = render_page(self.server.document, self.server.state)
            self._send(HTTPStatus.OK, page.encode(), "text/html; charset=utf-8")
        elif path == prefix + "style.css":
            self._send(HTTPStatus.OK, CSS.encode(), "text/css; charset=utf-8")
        elif path == prefix + "app.js":
            self._send(HTTPStatus.OK, JS.encode(), "text/javascript; charset=utf-8")
        else:
            self._send(HTTPStatus.NOT_FOUND, b"Not found", "text/plain; charset=utf-8")

    def do_POST(self) -> None:
        if urlparse(self.path).path != f"/{self.server.token}/action":
            self._send(
                HTTPStatus.NOT_FOUND,
                b'{"ok":false,"error":"Not found"}',
                "application/json",
            )
            return
        if not self.server.begin_action():
            self._send(
                HTTPStatus.SERVICE_UNAVAILABLE,
                b'{"ok":false,"error":"Review is closing"}',
                "application/json; charset=utf-8",
            )
            return
        try:
            self._handle_action()
        finally:
            self.server.end_action()

    def _handle_action(self) -> None:
        try:
            length = int(self.headers.get("Content-Length", "-1"))
            if length < 0 or length > MAX_ACTION_BYTES:
                raise ValueError("action body exceeds limit")
            value = json.loads(self.rfile.read(length))
            if (
                not isinstance(value, dict)
                or not set(value).issubset({"action", "section", "comment"})
                or not isinstance(value.get("action"), str)
            ):
                raise ValueError("action request is invalid")
            comment = value.get("comment", "")
            if (
                not isinstance(comment, str)
                or len(comment.encode()) > MAX_COMMENT_BYTES
            ):
                raise ValueError("review comment exceeds 4 KiB")
            for key in ("section",):
                if key in value and not isinstance(value[key], str):
                    raise ValueError("action request is invalid")
            result = validate_result(
                self.server.bridge.request(value), self.server.document
            )
            with self.server.state_lock:
                self.server.state = result["state"]
            status = HTTPStatus.OK if result.get("ok") else HTTPStatus.BAD_REQUEST
            self._send(
                status,
                json.dumps(result, separators=(",", ":")).encode(),
                "application/json; charset=utf-8",
            )
        except (TypeError, ValueError, json.JSONDecodeError) as error:
            self._send(
                HTTPStatus.BAD_REQUEST,
                json.dumps({"ok": False, "error": str(error)}).encode(),
                "application/json; charset=utf-8",
            )


def serve(reader: BinaryIO, writer: TextIO, open_browser: Callable[[str], Any]) -> int:
    descriptor = validate_init(read_json_line(reader, MAX_INIT_LINE_BYTES))
    server = ReviewServer(("127.0.0.1", 0), Handler)
    server.document = descriptor["document"]
    server.state = descriptor["state"]
    server.token = secrets.token_urlsafe(32)
    server.state_lock = threading.Lock()
    server.action_condition = threading.Condition()
    server.active_actions = 0
    server.shutting_down = False
    server.shutdown_thread = None
    host, port = server.server_address
    url = f"http://{host}:{port}/{server.token}/"
    bridge = Bridge(
        reader,
        writer,
        lambda: open_browser(url),
        server.request_shutdown,
    )
    server.bridge = bridge
    writer.write(
        json.dumps({"type": "ready", "url": url}, separators=(",", ":")) + "\n"
    )
    writer.flush()
    bridge.start()
    try:
        server.serve_forever()
    finally:
        server.server_close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-open", action="store_true", help="do not open a browser")
    args = parser.parse_args()
    opener: Callable[[str], Any] = (
        (lambda _url: None)
        if args.no_open
        else (lambda url: webbrowser.open(url, new=2))
    )
    try:
        return serve(sys.stdin.buffer, sys.stdout, opener)
    except (EOFError, TypeError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
