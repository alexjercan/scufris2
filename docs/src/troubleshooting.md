# Troubleshooting

## Popup evaluation fails

`voice.popup.enable` requires both `programs.scufris.enable` and `voice.enable`. Voice and the popup support Linux only.

## Piper assertion fails

Voice requires the private Piper 1.4.2 interface. An override with another version is rejected. The configuration path must equal the model path plus `.json`.

## Popup service does not start automatically

This is expected. Scufris defines the user service without an install target. The desktop configuration owns startup and toggle behavior. Start it explicitly for diagnosis:

```bash
systemctl --user start scufris-popup.service
systemctl --user status scufris-popup.service
```

## Speech produces no audio

Confirm that voice is enabled, then enable speech with `/speech on`. Check the user service log for Piper or PipeWire errors:

```bash
journalctl --user -u scufris-popup.service
```

Scufris reports speech failures without failing the completed assistant turn.

## Voice input does not work

Voice input comes from Pi configuration. Check the speech-to-text capture and endpoint there.

## Widget control fails

Dashboardd is external. Confirm that its desktop service and `dashboardctl` are available, or set `programs.scufris.widgets.enable = false`.

## Voice development rejects the environment

Run `npm run dev:voice` inside `nix develop`. The shell supplies Piper, PipeWire, and trusted model paths. Do not set mutable model paths as a substitute.

## Nix does not see a new documentation file

Flake source uses the Git worktree. Stage a new source file before a Nix build:

```bash
git add docs nix
nix build .#docs
```
