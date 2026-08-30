#!/usr/bin/env python3
import json
import socket
import time
from pathlib import Path

ROOT = Path("/run/user/1000/scufris-staging")
BUFFERS: dict[int, bytes] = {}


def open_channel(name: str) -> socket.socket:
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.settimeout(10)
    connection.connect(str(ROOT / name))
    return connection


def send(connection: socket.socket, value: object) -> None:
    connection.sendall(json.dumps(value, separators=(",", ":")).encode() + b"\n")


def receive(connection: socket.socket, timeout: float = 10) -> dict:
    connection.settimeout(timeout)
    key = connection.fileno()
    buffered = BUFFERS.get(key, b"")
    while b"\n" not in buffered:
        part = connection.recv(65536)
        if not part:
            raise EOFError("connection closed")
        buffered += part
    line, rest = buffered.split(b"\n", 1)
    BUFFERS[key] = rest
    return json.loads(line)


def register(surface: str) -> tuple[socket.socket, list[dict]]:
    connection = open_channel("surface.sock")
    send(
        connection,
        {
            "v": 4,
            "type": "surface.hello",
            "surface": {"id": surface, "name": surface, "widgets": []},
        },
    )
    replay = []
    while True:
        message = receive(connection)
        replay.append(message)
        if message["type"] == "surface.ready":
            return connection, replay


result: dict[str, object] = {}

wrong = open_channel("surface.sock")
send(
    wrong,
    {
        "v": 3,
        "type": "surface.hello",
        "surface": {"id": "wrong", "name": "wrong", "widgets": []},
    },
)
wrong.settimeout(5)
result["wrong_version_eof"] = wrong.recv(1) == b""
wrong.close()

violation = open_channel("control.sock")
send(violation, {"v": 4, "type": "agent.hello"})
violation.settimeout(5)
result["channel_violation_eof"] = violation.recv(1) == b""
violation.close()

second_agent = open_channel("agent.sock")
send(second_agent, {"v": 4, "type": "agent.hello"})
result["second_agent"] = receive(second_agent)
second_agent.close()

one, replay_one = register("synthetic-one")
two, replay_two = register("synthetic-two")
result["replay_one_types"] = [message["type"] for message in replay_one]
result["replay_two_types"] = [message["type"] for message in replay_two]

send(
    one,
    {
        "v": 4,
        "type": "surface.message",
        "id": "proof-1",
        "text": "Reply with one short sentence confirming protocol v4.",
    },
)
seen_one: list[dict] = []
seen_two: list[dict] = []
deadline = time.monotonic() + 120
assistant_one = None
assistant_two = None
while time.monotonic() < deadline and (assistant_one is None or assistant_two is None):
    for connection, seen, key in [(one, seen_one, "one"), (two, seen_two, "two")]:
        try:
            message = receive(connection, 1)
        except TimeoutError:
            continue
        seen.append(message)
        if (
            message.get("type") == "surface.message"
            and message.get("role") == "assistant"
        ):
            if key == "one":
                assistant_one = message
            else:
                assistant_two = message

result["live_one"] = seen_one
result["live_two"] = seen_two
result["assistant_identical"] = (
    assistant_one == assistant_two and assistant_one is not None
)
result["assistant_surface"] = assistant_one.get("surface") if assistant_one else None

one.close()
two.close()
print(json.dumps(result, indent=2, sort_keys=True))
if not all(
    [
        result["wrong_version_eof"],
        result["channel_violation_eof"],
        result["assistant_identical"],
        result["assistant_surface"] == "synthetic-one",
    ]
):
    raise SystemExit(1)
