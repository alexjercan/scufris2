#!/usr/bin/env python3
"""Measure the six Phase 0 Pi behaviour gates of the terminal handoff design.

Each gate is one question about Pi 0.85 that the design's shape depends on and
that the installed documentation does not answer. Every gate here runs the real
`pi` binary against a disposable agent directory, session directory, and
working tree, so what is recorded is behaviour rather than expectation.

Usage: python3 run_gates.py [--out DIR] [--only G1,G4]

Results land in `<out>/results.json` and the per-gate traces, session files, and
terminal captures stay beside them for inspection.
"""

from __future__ import annotations

import argparse
import json
import os
import pty
import re
import select
import shutil
import subprocess
import sys
import time
from pathlib import Path

PROBE = Path(__file__).resolve().parent / "probe.ts"
GATE_NOTICE_TEXT = "gate probe cancelled the session switch"
ANSI = re.compile(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b[()][A-Z0-9]|\x1b[=>]|\r")


class Harness:
    """One disposable Pi installation, session directory, and project."""

    def __init__(self, root: Path) -> None:
        self.root = root
        self.agent_dir = root / "pi-agent"
        self.sessions = root / "sessions"
        self.project = root / "project"
        self.home = root / "home"
        for directory in (self.agent_dir, self.sessions, self.project, self.home):
            directory.mkdir(parents=True, exist_ok=True)
        deployed = Path(
            os.environ.get("PI_CODING_AGENT_DIR", Path.home() / ".pi/agent")
        )
        auth = deployed / "auth.json"
        if auth.exists() and not (self.agent_dir / "auth.json").exists():
            (self.agent_dir / "auth.json").symlink_to(auth)
        settings = deployed / "settings.json"
        target = self.agent_dir / "settings.json"
        if settings.exists() and not target.exists():
            target.write_text(settings.read_text())
            target.chmod(0o600)

    def env(self, **extra: str) -> dict[str, str]:
        environment = dict(os.environ)
        environment["PI_CODING_AGENT_DIR"] = str(self.agent_dir)
        environment["HOME"] = str(self.home)
        environment["PI_OFFLINE"] = "0"
        environment.pop("SCUFRIS_GATE_LOG", None)
        for key in list(environment):
            if key.startswith("SCUFRIS_GATE_"):
                environment.pop(key)
        environment.update(extra)
        return environment


def base_flags(sessions: Path) -> list[str]:
    return [
        "--no-skills",
        "--no-context-files",
        "--no-prompt-templates",
        "--thinking",
        "off",
        "--session-dir",
        str(sessions),
    ]


def read_session(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def header(path: Path) -> dict:
    entries = read_session(path)
    return entries[0] if entries else {}


def newest(sessions: Path, exclude: set[Path] = frozenset()) -> Path | None:
    files = [f for f in sessions.glob("*.jsonl") if f not in exclude]
    if not files:
        return None
    return max(files, key=lambda f: f.stat().st_mtime)


def trace(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def run_print(
    harness: Harness,
    cwd: Path,
    args: list[str],
    prompt: str,
    timeout: int = 180,
    **env: str,
):
    return subprocess.run(
        ["pi", *args, "-p", prompt],
        cwd=cwd,
        env=harness.env(**env),
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )


def run_rpc(
    harness: Harness,
    cwd: Path,
    args: list[str],
    commands: list[dict],
    settle: float = 60.0,
    **env: str,
):
    """Drive one `pi --mode rpc` child and return every event it emitted."""
    process = subprocess.Popen(
        ["pi", "--mode", "rpc", *args],
        cwd=cwd,
        env=harness.env(**env),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    events: list[dict] = []
    deadline = time.monotonic() + settle
    try:
        for command in commands:
            process.stdin.write(json.dumps(command) + "\n")
            process.stdin.flush()
        idle_since = None
        while time.monotonic() < deadline:
            ready, _, _ = select.select([process.stdout], [], [], 0.5)
            if ready:
                line = process.stdout.readline()
                if not line:
                    break
                idle_since = None
                try:
                    events.append(json.loads(line))
                except json.JSONDecodeError:
                    events.append({"type": "__raw", "line": line.rstrip()})
                continue
            if any(event.get("type") == "agent_settled" for event in events):
                idle_since = idle_since or time.monotonic()
                if time.monotonic() - idle_since > 2.0:
                    break
    finally:
        try:
            process.stdin.close()
        except OSError:
            # The child is already gone, which is what this was making sure of.
            pass
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    return events


def run_tui(
    harness: Harness,
    cwd: Path,
    args: list[str],
    script: list[tuple[float, str]],
    settle: float = 20.0,
    **env: str,
) -> str:
    """Drive the real TUI through a pseudo-terminal and return what it drew.

    `script` is a list of (wait seconds, bytes to type). The wait happens
    before the keys are sent, so a step can let a turn finish first.
    """
    primary, secondary = pty.openpty()
    os.set_blocking(primary, False)
    environment = harness.env(**env)
    environment["TERM"] = "xterm-256color"
    environment["COLUMNS"] = "100"
    environment["LINES"] = "40"
    process = subprocess.Popen(
        ["pi", *args, "--tui-mode", "regular"],
        cwd=cwd,
        env=environment,
        stdin=secondary,
        stdout=secondary,
        stderr=secondary,
        start_new_session=True,
    )
    os.close(secondary)
    captured = bytearray()

    def pump(seconds: float) -> None:
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            ready, _, _ = select.select([primary], [], [], 0.2)
            if not ready:
                continue
            try:
                chunk = os.read(primary, 65536)
            except OSError:
                return
            if not chunk:
                return
            captured.extend(chunk)

    try:
        for wait, keys in script:
            pump(wait)
            if keys:
                os.write(primary, keys.encode())
        pump(settle)
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        pump(0.5)
        os.close(primary)
    return ANSI.sub("", captured.decode("utf8", "replace"))


def seed(harness: Harness, gate_root: Path, word: str) -> Path:
    """One base session with a fact in it, written from the project cwd.

    Print mode stalls now and then for reasons outside this harness, so a
    stalled seed is retried once rather than failing the gate it only sets up.
    """
    prompt = f"Remember this code word: {word}. Reply with only: OK"
    attempts = []
    for attempt in range(2):
        before = set(harness.sessions.glob("*.jsonl"))
        try:
            result = run_print(
                harness,
                harness.project,
                base_flags(harness.sessions) + ["--no-extensions"],
                prompt,
                timeout=180,
            )
        except subprocess.TimeoutExpired:
            attempts.append(f"attempt {attempt + 1}: timed out")
            for stale in set(harness.sessions.glob("*.jsonl")) - before:
                stale.unlink()
            continue
        if result.returncode != 0:
            attempts.append(f"attempt {attempt + 1}: {result.stderr[-500:]}")
            continue
        file = newest(harness.sessions)
        assert file is not None
        (gate_root / "seed.stdout").write_text(result.stdout)
        (gate_root / "seed.log").write_text("\n".join(attempts))
        return file
    raise RuntimeError("seeding failed: " + "; ".join(attempts))


# --- G1 ---------------------------------------------------------------------


def gate_g1(harness: Harness, out: Path) -> dict:
    """Does `--session-dir D --fork FILE` plus a typed turn append to the new
    file, in TUI and in RPC mode?"""
    base = seed(harness, out, "ALPHA-41")
    terminal = harness.root / "terminal"
    terminal.mkdir(exist_ok=True)
    before = set(harness.sessions.glob("*.jsonl"))
    question = "What is the code word I gave you? Reply with only the word."

    rpc_events = run_rpc(
        harness,
        terminal,
        base_flags(harness.sessions) + ["--no-extensions", "--fork", str(base)],
        [{"id": "p1", "type": "prompt", "message": question}],
    )
    (out / "g1-rpc-events.json").write_text(json.dumps(rpc_events, indent=2))
    rpc_file = newest(harness.sessions, before)
    rpc = {"file": str(rpc_file) if rpc_file else None}
    if rpc_file:
        entries = read_session(rpc_file)
        head = entries[0]
        users = [
            e
            for e in entries
            if e.get("type") == "message" and e["message"]["role"] == "user"
        ]
        assistants = [
            e
            for e in entries
            if e.get("type") == "message" and e["message"]["role"] == "assistant"
        ]
        rpc.update(
            {
                "header": head,
                "parent_is_base": head.get("parentSession") == str(base),
                "cwd": head.get("cwd"),
                "cwd_is_terminal": head.get("cwd") == str(terminal),
                "copied_entry_types": [e.get("type") for e in read_session(base)[1:]],
                "user_messages": len(users),
                "turn_appended": any(question in json.dumps(e) for e in users),
                "recalls": "ALPHA-41" in json.dumps(assistants),
            }
        )

    before = set(harness.sessions.glob("*.jsonl"))
    log = out / "g1-tui.jsonl"
    capture = run_tui(
        harness,
        terminal,
        base_flags(harness.sessions) + ["--extension", str(PROBE), "--fork", str(base)],
        [(8.0, f"{question}\r"), (25.0, "")],
        settle=5.0,
        SCUFRIS_GATE_LOG=str(log),
    )
    (out / "g1-tui.capture").write_text(capture)
    tui_file = newest(harness.sessions, before)
    tui = {"file": str(tui_file) if tui_file else None, "trace": trace(log)}
    if tui_file:
        entries = read_session(tui_file)
        head = entries[0]
        assistants = [
            e
            for e in entries
            if e.get("type") == "message" and e["message"]["role"] == "assistant"
        ]
        tui.update(
            {
                "header": head,
                "parent_is_base": head.get("parentSession") == str(base),
                "cwd_is_terminal": head.get("cwd") == str(terminal),
                "turn_appended": question in json.dumps(entries),
                "recalls": "ALPHA-41" in json.dumps(assistants),
                "stalled": not assistants,
            }
        )
    passed = bool(
        rpc.get("parent_is_base")
        and rpc.get("turn_appended")
        and rpc.get("cwd_is_terminal")
        and tui.get("parent_is_base")
        and tui.get("turn_appended")
        and tui.get("cwd_is_terminal")
    )
    return {"gate": "G1", "pass": passed, "base": str(base), "rpc": rpc, "tui": tui}


# --- G2 ---------------------------------------------------------------------


def gate_g2(harness: Harness, out: Path) -> dict:
    """Does `--continue --session-dir D` from the home cwd resume the file whose
    header cwd is home and skip a newer file written from the project cwd?"""
    flags = base_flags(harness.sessions) + ["--no-extensions"]
    home_run = run_print(harness, harness.home, flags, "Reply with only: HOME")
    home_file = newest(harness.sessions)
    time.sleep(1.1)
    project_run = run_print(harness, harness.project, flags, "Reply with only: PROJECT")
    project_file = newest(harness.sessions)
    (out / "g2-seed.txt").write_text(
        f"home={home_file}\nproject={project_file}\n{home_run.stdout}{project_run.stdout}"
    )
    before = {p: p.stat().st_size for p in harness.sessions.glob("*.jsonl")}
    resumed = run_print(
        harness, harness.home, flags + ["--continue"], "Reply with only: RESUMED"
    )
    grown = [str(path) for path, size in before.items() if path.stat().st_size > size]
    created = [str(p) for p in harness.sessions.glob("*.jsonl") if p not in before]
    return {
        "gate": "G2",
        "pass": grown == [str(home_file)] and not created,
        "home_file": str(home_file),
        "home_cwd": header(home_file).get("cwd"),
        "project_file": str(project_file),
        "project_cwd": header(project_file).get("cwd"),
        "grown": grown,
        "created": created,
        "stdout": resumed.stdout.strip(),
    }


# --- G3 ---------------------------------------------------------------------


def gate_g3(harness: Harness, out: Path) -> dict:
    """Can a `session_start` handler dispatch an extension command through
    `sendUserMessage(..., {expandPromptTemplates: true})` and have that command
    call `ctx.switchSession` without deadlocking?"""
    target = seed(harness, out, "BRAVO-72")
    runs = {}
    for name, guard in (("unguarded", "0"), ("guarded", "1")):
        log = out / f"g3-{name}.jsonl"
        started = time.monotonic()
        capture = run_tui(
            harness,
            harness.project,
            base_flags(harness.sessions) + ["--extension", str(PROBE)],
            [(20.0, "")],
            settle=5.0,
            SCUFRIS_GATE_LOG=str(log),
            SCUFRIS_GATE_DISPATCH="/gateattach",
            SCUFRIS_GATE_DISPATCH_ONCE=guard,
            SCUFRIS_GATE_SWITCH_TARGET=str(target),
        )
        (out / f"g3-{name}.capture").write_text(capture)
        entries = trace(log)
        events = [entry["event"] for entry in entries]
        runs[name] = {
            "switches": events.count("gateattach_withsession"),
            "completed": events.count("gateattach_done"),
            "failed": events.count("gateattach_failed"),
            "session_starts": events.count("session_start"),
            "resumed": any(
                e["event"] == "session_start" and e.get("reason") == "resume"
                for e in entries
            ),
            "seconds": round(time.monotonic() - started, 1),
            "first_events": events[:8],
        }
    return {
        "gate": "G3",
        "pass": runs["guarded"]["switches"] == 1
        and runs["guarded"]["resumed"]
        and runs["guarded"]["failed"] == 0,
        "runs": runs,
        "target": str(target),
    }


# --- G4 ---------------------------------------------------------------------


def gate_g4(harness: Harness, out: Path) -> dict:
    """Do built-in commands, extension commands, and `!` lines fire `input`?"""
    log = out / "g4.jsonl"
    capture = run_tui(
        harness,
        harness.project,
        base_flags(harness.sessions) + ["--extension", str(PROBE)],
        [
            (8.0, "/gatecmd\r"),
            (3.0, "!echo gate-bang-ran\r"),
            (4.0, "/model\r"),
            (4.0, "\x1b"),
            (3.0, "/skill:nothing-here\r"),
            (3.0, "Reply with only: TYPED\r"),
            (25.0, ""),
        ],
        settle=5.0,
        SCUFRIS_GATE_LOG=str(log),
    )
    (out / "g4.capture").write_text(capture)
    entries = trace(log)
    inputs = [e for e in entries if e["event"] == "input"]
    texts = [e["text"] for e in inputs]
    return {
        "gate": "G4",
        "pass": not any(t.startswith(("/gatecmd", "/model", "!")) for t in texts)
        and any(
            t == "Reply with only: TYPED" and e["source"] == "interactive"
            for t, e in zip(texts, inputs)
        ),
        "inputs": inputs,
        "extension_command_ran": any(e["event"] == "command_gatecmd" for e in entries),
        "bang_ran": "gate-bang-ran" in capture,
        "skill_line_reached_input": any(t.startswith("/skill:") for t in texts),
    }


# --- G5 ---------------------------------------------------------------------


def gate_g5(harness: Harness, out: Path) -> dict:
    """Does a cancelling `session_before_switch` stop `/new` and `/resume` in
    the TUI, with the notice visible?"""
    seed(harness, out, "DELTA-19")
    log = out / "g5.jsonl"
    before = set(harness.sessions.glob("*.jsonl"))
    capture = run_tui(
        harness,
        harness.project,
        base_flags(harness.sessions) + ["--extension", str(PROBE)],
        [
            (8.0, "Reply with only: ONE\r"),
            (25.0, "/new\r"),
            (6.0, "/resume\r"),
            (6.0, "\r"),
            (6.0, "\x1b"),
            (3.0, ""),
        ],
        settle=6.0,
        SCUFRIS_GATE_LOG=str(log),
        SCUFRIS_GATE_CANCEL_SWITCH="1",
    )
    (out / "g5.capture").write_text(capture)
    entries = trace(log)
    reasons = [e.get("reason") for e in entries if e["event"] == "switch_cancelled"]
    created = [str(p) for p in harness.sessions.glob("*.jsonl") if p not in before]
    starts = [e for e in entries if e["event"] == "session_start"]
    return {
        "gate": "G5",
        "pass": "new" in reasons
        and "resume" in reasons
        and len(starts) == 1
        and len(created) == 1
        and GATE_NOTICE_TEXT in capture,
        "cancelled_reasons": reasons,
        "notice_drawn": GATE_NOTICE_TEXT in capture,
        "session_starts": len(starts),
        "created": created,
        "trace": entries,
    }


# --- G6 ---------------------------------------------------------------------


def gate_g6(harness: Harness, out: Path) -> dict:
    """Does a `display: false` custom message injected at `session_start` reach
    the model on the next turn?"""
    results = {}
    for mode in ("nextTurn", "steer"):
        log = out / f"g6-{mode}.jsonl"
        before = set(harness.sessions.glob("*.jsonl"))
        events = run_rpc(
            harness,
            harness.project,
            base_flags(harness.sessions) + ["--extension", str(PROBE)],
            [
                {
                    "id": "p1",
                    "type": "prompt",
                    "message": "What is the pass phrase? Reply with only the phrase.",
                }
            ],
            SCUFRIS_GATE_LOG=str(log),
            SCUFRIS_GATE_CATCH_UP=f"Earlier in this conversation the pass phrase was set to CHARLIE-{mode.upper()}.",
            SCUFRIS_GATE_CATCH_UP_MODE=mode,
        )
        (out / f"g6-{mode}-events.json").write_text(json.dumps(events, indent=2))
        file = newest(harness.sessions, before)
        answer = json.dumps(
            [
                e
                for e in (read_session(file) if file else [])
                if e.get("type") == "message" and e["message"]["role"] == "assistant"
            ]
        )
        results[mode] = {
            "injected": any(e["event"] == "catch_up_injected" for e in trace(log)),
            "custom_in_session": any(
                e.get("customType") == "scufris-gate-catch-up"
                for e in (read_session(file) if file else [])
            ),
            "recalled": f"CHARLIE-{mode.upper()}" in answer,
            "file": str(file) if file else None,
        }
    return {
        "gate": "G6",
        "pass": any(value["recalled"] for value in results.values()),
        "modes": results,
    }


GATES = {
    "G1": gate_g1,
    "G2": gate_g2,
    "G3": gate_g3,
    "G4": gate_g4,
    "G5": gate_g5,
    "G6": gate_g6,
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out", default="gate-run", help="directory for results and evidence"
    )
    parser.add_argument("--only", default="", help="comma-separated gate names")
    parser.add_argument(
        "--keep", action="store_true", help="reuse an existing output directory"
    )
    arguments = parser.parse_args()
    out = Path(arguments.out).resolve()
    if out.exists() and not arguments.keep:
        shutil.rmtree(out)
    out.mkdir(parents=True, exist_ok=True)
    wanted = [name for name in arguments.only.split(",") if name] or list(GATES)
    results = []
    for name in wanted:
        gate_out = out / name
        gate_out.mkdir(exist_ok=True)
        harness = Harness(gate_out / "harness")
        print(f"running {name} ...", flush=True)
        try:
            result = GATES[name](harness, gate_out)
        except Exception as error:  # noqa: BLE001 - a gate that cannot run is a failed gate
            result = {
                "gate": name,
                "pass": False,
                "error": f"{type(error).__name__}: {error}",
            }
        results.append(result)
        print(f"  {name}: {'PASS' if result.get('pass') else 'FAIL'}", flush=True)
        (out / "results.json").write_text(json.dumps(results, indent=2))
    return 0 if all(result.get("pass") for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
