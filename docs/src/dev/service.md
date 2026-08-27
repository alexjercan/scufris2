# Background service

`scufris-service` is the headless half of Scufris. It owns three things nothing
else may own: one `pi --mode rpc` agent, the session directory that agent
writes, and the socket every surface connects to. The voice pill, the tray, a
terminal and `scufris-ctl` are all clients of it.

It builds and runs with no graphical dependency, so a machine with no display
keeps the conversation and a terminal over ssh reaches it.

> The companion still serves its own version 2 socket for the popup Pi process.
> The two arrangements stand beside each other; nothing graphical uses the
> service yet.

## What it owns

- **One agent.** Started as
  `<agent> --session-dir <dir> --continue --mode rpc`, in its own process
  group, with `SCUFRIS_DAEMON` stripped so it never becomes a version 2 server
  of its own. Stopping it is a sequence: stdin closes first, because that is
  how an RPC client says goodbye, then `SIGTERM`, then `SIGKILL`, then the exit
  status is collected.
- **One session directory.** `--continue` rather than a named session, so the
  newest session in the directory is the conversation, including right after a
  terminal handed it back.
- **One socket.** `$XDG_RUNTIME_DIR/scufris/service.sock`, under a private
  directory, mode 0600. A socket that still answers is another service and the
  bind fails; a socket with nothing behind it is what a crash leaves, so that
  one is removed.
- **The last screenful of the conversation.** A ring of 200 text entries, so a
  frontend that connects has something to show before the next thing is said.

## The socket

LF-terminated JSON, one message per line, both ways. Every message carries
`"v": 3`, and a peer that speaks another version is told which version it
spoke rather than that its message did not parse.

The first message on a connection is always `hello`, and there is never a
second one:

```
{"v":3,"type":"hello","role":"control"}  ->  {"v":3,"type":"welcome","role":"control"}
```

Two roles. `frontend` is a surface: it submits text and is pushed every state
change and every transcript entry. `control` is `scufris-ctl`: it asks one
thing, reads the answer, and is pushed nothing.

Requests carry an `id` and answers echo it, so a client that pipelines two
requests tells the answers apart without counting.

| Request                                     | Answer                                              |
| ------------------------------------------- | --------------------------------------------------- |
| `{"type":"submit","id":"1","text":"hello"}` | `{"type":"ok","id":"1"}`                            |
| `{"type":"abort","id":"2"}`                 | `{"type":"ok","id":"2"}`                            |
| `{"type":"get_state","id":"3"}`             | `{"type":"state","id":"3","state":"idle",...}`      |
| `{"type":"debug","id":"4"}`                 | `{"type":"debug","id":"4","program":...,"args":[]}` |

Anything the service will not do comes back as
`{"type":"refused","id":...,"code":...,"detail":...}`. The codes are stable and
are what a caller branches on: `agent_unavailable`, `detached`, `debug_held`,
`wrong_role`, `agent_refused`. `detail` is for a person to read.

A submission while the agent is working is delivered as a steer, not refused.

## State

`starting`, `idle`, `working`, `detached`, `error`. That is the whole
vocabulary, and it is deliberately small: a frontend never parses a Pi event
and never learns Pi's vocabulary, so an event Pi adds tomorrow is a service
change and nothing else. Speaking and listening are not in it, because those
are what a frontend is doing rather than what Scufris is doing.

A frontend is pushed `state` whenever it changes, and `transcript` for each
line that was said. Both arrive unasked and neither carries an `id`.

## The debug lease

`scufris-ctl debug` takes the agent away from the service and opens its session
in the terminal that asked:

```
scufris-ctl debug
```

The service stops its agent, reports `detached`, and answers with the command
line that resumes the same session - the exact session file the agent named, or
the directory and `--continue` before it has named one. The client runs it and
waits.

There is deliberately no `detach` and no `attach`. A pair of verbs is a
sequence to remember and a state to get stuck in. **The lease is the
connection.** When it closes - a clean exit, a Ctrl-C, a closed terminal, a
kill, a crashed client - the service starts the agent again. There is no way to
be left detached with nothing to put it back. A second `debug` while a lease is
held is refused with `debug_held`.

The client blocks `SIGINT` and `SIGQUIT`, but only _after_ spawning `pi`: the
signal mask is inherited across `exec`, so blocking first would leave `pi`
unable to see its own Ctrl-C.

## The client

`scufris-ctl` is its own flake package because a window manager binding runs it
by name and a terminal reaches the service with it, and neither wants the other
half of Scufris installed beside it.

```
scufris-ctl send <words...>   say something to Scufris
scufris-ctl state             print what Scufris is doing
scufris-ctl watch             follow the state and the conversation
scufris-ctl abort             end the run that is in progress
scufris-ctl debug             take the agent and open its session here
```

`open`, `cancel` and `accept` still go to the companion's own socket. They go
away with the pill's keys. See [Desktop companion](desktop.md).

Exit status is what a binding branches on without parsing anything: 0 it
worked, 1 it did not, 2 the run was wrong. `debug` exits with whatever `pi`
exited with.

## Configuration

Two settings, each an option with an environment variable behind it. Nothing is
read from a configuration file: a second place to say which `pi` this is would
be a second place for it to be wrong.

| Option          | Variable                      | Default                           |
| --------------- | ----------------------------- | --------------------------------- |
| `--agent`       | `SCUFRIS_SERVICE_AGENT`       | the first `scufris` on `PATH`     |
| `--session-dir` | `SCUFRIS_SERVICE_SESSION_DIR` | `$XDG_DATA_HOME/scufris/sessions` |

Both must be absolute; a relative path is refused rather than resolved against
the working directory. `RUST_LOG` sets the log directives. Logs go to journald
under systemd and to stderr from a terminal.

## Running it by hand

```bash
cd native
nix develop --offline -c cargo build -p scufris-service
SCUFRIS_SERVICE_AGENT="$(command -v scufris)" ./target/debug/scufris-service
```

Then, from another terminal, `./target/debug/scufris-ctl state`.

## Packaging

`nix/service.nix` builds `-p scufris-service` out of the `native/` workspace and
splits the result into two packages, `scufris-service` and `scufris-ctl`, from
one build. Neither pulls in GTK or WebKitGTK, which `nix/checks/service.nix`
asserts against their closures.

`programs.scufris.service.enable` gives the service a systemd user unit of its
own, wanted by `default.target` rather than by `graphical-session.target`. It is
off by default. The module installs `scufris-ctl` beside whichever half is
enabled, and it is one package, so enabling both does not collide.
