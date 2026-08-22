# Home Manager

Import the module from the pinned flake input:

```nix
{
  inputs,
  ...
}: {
  imports = [inputs.scufris.homeModules.default];

  programs.scufris.enable = true;
}
```

This installs the rendered normal launcher in `home.packages`. Delegation and widget control are enabled by default. Configure their enable options when the matching external service is not available.

## Compose with a configured Pi package

A configuration that also manages Pi can use its final package:

```nix
{
  config,
  inputs,
  ...
}: {
  imports = [inputs.scufris.homeModules.default];

  programs.scufris = {
    enable = true;
    piPackage = config.programs.pi.coding-agent.finalPackage;
  };
}
```

## Enable voice

```nix
programs.scufris = {
  enable = true;
  voice.enable = true;
};
```

Voice requires Linux. It selects the voice resources, private patched Piper 1.4.2 package, pinned model, and PipeWire playback runtime.

## Define the popup service

```nix
programs.scufris = {
  enable = true;
  voice = {
    enable = true;
    popup.enable = true;
  };
};
```

The module defines `scufris-popup.service` but does not install it into a target or start it. A desktop consumer can use the read-only service identity and final launcher options to implement startup and toggle policy.

See the [generated option reference](reference/options.md) for evaluated types, defaults, descriptions, and all supported overrides.
