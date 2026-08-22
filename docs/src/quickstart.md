# Quickstart

## Prerequisite

Install Nix with flakes enabled.

## Run a release

Run the normal package from the current release tag:

```bash
nix run github:alexjercan/scufris2/v0.2.0#scufris
```

Run the voice-capable package on Linux:

```bash
nix run github:alexjercan/scufris2/v0.2.0#scufris-voice
```

The voice-capable package starts silent. Use `/speech on` for persistent speech, `/speech once` for one response, and `/speech replay` to repeat the last safe paragraph.

## Run a checkout

```bash
git clone https://github.com/alexjercan/scufris2.git
cd scufris2
nix run .#scufris
```

Use Home Manager for persistent installation and popup configuration.
