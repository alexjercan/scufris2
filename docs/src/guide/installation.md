# Installation

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

The voice-capable package starts silent. Enable speech inside the session with
`/speech on`.

## Run a checkout

```bash
git clone https://github.com/alexjercan/scufris2.git
cd scufris2
nix run .#scufris
```

## Flake interface

Pin a release tag and share `nixpkgs` with the parent flake:

```nix
{
  inputs.scufris = {
    url = "github:alexjercan/scufris2/v0.2.0";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Release tags are immutable inputs. Update the tag deliberately.

Packages:

- `default` and `scufris`: the normal launcher.
- `scufris-voice`: the Linux-only voice-capable launcher.
- `resources`: extensions, skills, and deterministic tools without speech.
- `voice-resources`: resources that also contain speech playback and its tool.
- `docs`: this manual, including the generated option reference.

Resource packages are composition inputs. Most users need only a launcher or
the Home Manager module.

## Home Manager

Import the module from the pinned flake input:

```nix
{inputs, ...}: {
  imports = [inputs.scufris.homeModules.default];

  programs.scufris.enable = true;
}
```

This installs the rendered launcher in `home.packages`. The workflow,
response, and Calm extensions are always present. Dashboard control is enabled
by default; set `programs.scufris.dashboard.enable = false` when Dashboardd is
unavailable.

A configuration that also manages Pi can pass its final package:

```nix
programs.scufris = {
  enable = true;
  piPackage = config.programs.pi.coding-agent.finalPackage;
};
```

Set `programs.scufris.projectRoots` to control which directories Scufris
searches for workflow projects. The default is `~/personal`, `~/work`, and
`~/third-party`.

## Voice and the popup

```nix
programs.scufris = {
  enable = true;

  voice = {
    enable = true;
    popup.enable = true;
  };
};
```

Voice requires Linux. It selects the voice resources, a private patched Piper
1.4.2 package, the pinned `en_US-lessac-medium` model, and PipeWire playback.
Overrides must keep Piper version 1.4.2 and an immutable store model with its
configuration adjacent as `model.onnx.json`.

The popup defines the `scufris-popup.service` user service and a direct Kitty
launcher that resumes a dedicated session with speech and Calm enabled. The
module does not install the service into a target or start it. The desktop
configuration starts and presents it using the read-only `serviceName` and
`finalLauncher` options.

See the [option reference](../reference/options.md) for evaluated types,
defaults, and descriptions.
