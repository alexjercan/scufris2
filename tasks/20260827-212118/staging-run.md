# Staging run, 2026-08-27

The real binaries from the flake, on the live session:
`nix run .#staging -- up`, reached with `scufris-ctl` through
`SCUFRIS_RUNTIME_DIR`, then Ctrl+C.

## Before

```text
$ ls /run/user/1000/scufris /run/user/1000/scufris-staging
/run/user/1000/scufris:
daemon.sock.lock
desktop.sock
ls: cannot access '/run/user/1000/scufris-staging': No such file or directory

$ ls /tmp/scufris-staging
ls: cannot access '/tmp/scufris-staging': No such file or directory

$ systemctl --user is-active scufris-service.service scufris-desktop.service
inactive
inactive

$ stat -c "%n %y" ~/.local/state/scufris ~/.local/share/scufris ~/.pi/agent/settings.json
/home/alex/.local/state/scufris 2026-08-23 21:26:36.170984803 +0300
/home/alex/.local/share/scufris 2026-08-27 10:13:57.702763913 +0300
/home/alex/.pi/agent/settings.json 2026-08-26 09:55:14.176109393 +0300

```

## Up

```text
$ nix run .#staging -- up
warning: Git tree '/home/alex/personal/scufris2' is dirty
this derivation will be built:
  /nix/store/kbvzcw1i5n7dpwswar0ana62zfs5gr8m-scufris-staging.drv
building '/nix/store/kbvzcw1i5n7dpwswar0ana62zfs5gr8m-scufris-staging.drv'...
scufris-staging: seeded /tmp/scufris-staging/projects/hello
scufris-staging: seeded /tmp/scufris-staging/pi-agent
scufris-staging: the staging environment
  SCUFRIS_STAGING_ROOT=/tmp/scufris-staging
  XDG_STATE_HOME=/tmp/scufris-staging/state
  XDG_DATA_HOME=/tmp/scufris-staging/data
  SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-staging
  PI_CODING_AGENT_DIR=/tmp/scufris-staging/pi-agent
  SCUFRIS_PROJECT_ROOTS=["/tmp/scufris-staging/projects"]
  SCUFRIS_SERVICE_AGENT=/nix/store/d9zmk7kxkbpyg26dd9iz9fmkvrgy2kkn-source/scripts/scufris-agent
  SCUFRIS_DESKTOP_HOTKEY=Super+G
  SCUFRIS_STT_ENDPOINT=http://127.0.0.1:10301/inference
  service=/nix/store/nglg6qzrn1i0z9629d9a5s4m3jrbi6s3-scufris-service-0.4.0/bin/scufris-service
  desktop=/nix/store/3r31rgmnbinn9gnvyp0fay2aswahnnzk-scufris-desktop-0.4.0/bin/scufris-desktop

scufris-staging: reach it with
  SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-staging scufris-ctl state
scufris-staging: the companion resolves
  socket=/run/user/1000/scufris-staging/service.sock
  command_socket=/run/user/1000/scufris-staging/desktop.sock
  state_file=/tmp/scufris-staging/state/scufris-desktop/pending.json
  stt_endpoint=http://127.0.0.1:10301/inference
  hotkey=Super+G
  cancel_key=derived
  stop_key=derived
  chat_command=none
  restart_command=none
  speak_command=none
scufris-staging: service pid 1529186, desktop pid 1529187

(.scufris-desktop-wrapped:1529187): Gtk-WARNING **: 22:50:28.473: Unknown key Settings in /home/alex/.config/gtk-3.0/settings.ini

(.scufris-desktop-wrapped:1529187): Gdk-CRITICAL **: 22:50:28.832: gdk_window_thaw_toplevel_updates: assertion 'window->update_and_descendants_freeze_count > 0' failed
```

## Reaching it

```text
$ ls -l /run/user/1000/scufris-staging
total 0
srwx------ 1 alex users 0 Aug 27 22:50 desktop.sock
srw------- 1 alex users 0 Aug 27 22:50 service.sock

$ export SCUFRIS_RUNTIME_DIR=/run/user/1000/scufris-staging
$ scufris-ctl state
idle

$ scufris-ctl hud    # the conversation window up, then away

# without the override the same command reaches the deployed stack, which
# is not running, and says so rather than answering from staging
$ scufris-ctl state
scufris-ctl: the service is not listening on /run/user/1000/scufris/service.sock: No such file or directory (os error 2)
```

## An interaction

The `Super+G` half is a keypress and a microphone, so it is the one step
here that is not scripted. What the key drives after the transcription is
this, and the agent answering is a working-tree one: the service started
`scripts/scufris-agent`, which loads the extensions from this checkout.

The answer names the seeded staging project. The deployed project roots are
not in this agent's environment, which is the isolation working.

```text
$ scufris-ctl send what projects can you see
$ scufris-ctl watch
idle
user: what projects can you see
scufris: I can see one project: hello.
```

## Teardown

Ctrl+C, delivered as SIGINT to the foreground script. The trap stops the
two recorded PIDs and nothing else, and the run exits 0: an interrupt is
the teardown, not a failure.

```text
^C

scufris-staging: stopping

$ kill -0 1529186   # the service
bash: kill: (1529186) - No such process
$ kill -0 1529187   # the companion
bash: kill: (1529187) - No such process

$ echo $?   # the exit status of `up`
0
```

## After

```text
$ ls -A /run/user/1000/scufris /run/user/1000/scufris-staging
/run/user/1000/scufris:
daemon.sock.lock
desktop.sock

/run/user/1000/scufris-staging:
desktop.sock

$ ls /tmp/scufris-staging
data
pi-agent
projects
staging.lock
state

$ systemctl --user is-active scufris-service.service scufris-desktop.service
inactive
inactive

$ stat -c "%n %y" ~/.local/state/scufris ~/.local/share/scufris ~/.pi/agent/settings.json
/home/alex/.local/state/scufris 2026-08-23 21:26:36.170984803 +0300
/home/alex/.local/share/scufris 2026-08-27 10:13:57.702763913 +0300
/home/alex/.pi/agent/settings.json 2026-08-26 09:55:14.176109393 +0300

```

The deployed side is where it was: the same two units inactive, the same
three mtimes, and nothing new under `/run/user/1000/scufris`. The staging
runtime directory keeps `desktop.sock`, which the companion does not
unlink on the way out; the next run replaces it after finding nothing
answering on it.

## A second stack while one runs

The lock held by another process, which is what a running stack holds:

```text
$ scufris-staging up
scufris-staging: a staging stack is already running (/tmp/scufris-staging-second/staging.lock)
$ echo $?
3
```

It refuses before starting anything. The lock is released with the
process, so a crash leaves nothing to clear by hand.
`tests/test_scufris_staging.py` covers this against the real script with
stub binaries.
