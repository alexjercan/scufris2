# Flake interface

## Consume Scufris as an input

Pin a release tag and share `nixpkgs` with the parent flake:

```nix
{
  inputs.scufris = {
    url = "github:alexjercan/scufris2/v0.1.0";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Update the tag deliberately. Release tags are immutable inputs.

## Apps

- `apps.<system>.default` runs normal Scufris.
- `apps.<linux-system>.scufris-voice` runs voice-capable Scufris without enabling speech by default.

`nix run .#scufris` also runs the named package.

## Packages

- `default` and `scufris`: normal launcher.
- `scufris-voice`: Linux-only voice-capable launcher.
- `resources`: normal packaged extensions, skills, prompts, and helpers.
- `voice-resources`: resources that also contain the speech extension and helper.
- `docs`: this complete manual, including generated option documentation.

Resource packages are composition inputs. Most users need only a launcher or the Home Manager module.

## Module and component outputs

- `homeModules.default`: the `programs.scufris` Home Manager module.
- `extensions`: separately consumable Calm, speech, delegation, and widget extensions.
- `skills`: separately consumable delegation and widget skills.

The default module resolves its Pi and Dashboardd package defaults from the Scufris flake inputs. A parent configuration can override those packages through the documented options.
