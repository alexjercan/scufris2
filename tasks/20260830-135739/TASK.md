# Manage production Tailscale Serve declaratively

- STATUS: CLOSED
- PRIORITY: 100
- TAGS: nix, remote-surface

## Goal

Make `programs.scufris.service.remoteSurface.enable` own the complete private
WSS endpoint: the loopback protocol gateway and its Tailscale Serve root route.
Do not add a second proxy-selection option.

## Decisions

- The reusable Home Manager module owns production Tailscale Serve because
  Scufris remote surfaces use authenticated private WSS over the tailnet.
- Use a systemd oneshot unit to reconcile the background Tailscale route and
  remove exactly `/` on declarative stop. Foreground Serve cannot share its
  HTTPS listener with the independent staging path.
- Production owns only `/`; staging continues to own only
  `/scufris-staging`.
- Do not update `nix.dotfiles` and do not push commits for this request.

## Acceptance

- Enabling `remoteSurface` creates both the gateway and Serve user units.
- The Serve unit is ordered after the gateway, proxies the configured port at
  `/`, removes only `/` on stop, and retries failures.
- The module installs the pinned Tailscale client needed by the generated unit.
- Home Manager checks assert the exact unit interface and package closure.
- README, installation, operation, and changelog documentation describe the
  declarative ownership and Tailscale permission requirement.
- Focused checks and full `nix flake check` pass.

## Verification

- A live foreground Serve probe was rejected because foreground mode cannot
  share an HTTPS listener with the existing production route. This established
  the oneshot reconciliation design needed for production and staging paths to
  coexist.
- A live background-route proof on HTTPS port 8443 added `/` and a second path,
  removed exactly `/`, retained the second path, then removed the test listener.
  The production port-443 root route remained unchanged.
- Reapplying the exact production background route succeeded idempotently.
- `systemd-analyze --user verify` accepted the generated service, gateway, and
  Tailscale units.
- Focused Home Manager service checks and staging helper tests passed.
- The working-tree mdBook build passed.
- `nix flake check path:$PWD` passed all 40 compatible checks. The path form was
  used because a separate open documentation task has new files that are not
  yet tracked by Git.
