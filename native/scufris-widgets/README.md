# scufris-widgets

The panels the companion can put on the workspace, and the programs that feed
them. Two halves of one thing, so they live under one roof.

This is not a crate. It holds no Rust and is not a member of the workspace, and
the name matches its neighbours because it ships with them, not because it
builds anything of its own.

```
widgets/<id>/widget.toml    what it is called, how big, which backend
widgets/<id>/widget.ts      what it draws
widgets/widget.d.ts         the contract, and the only copy of it
backends/<id>/backend.py    one JSON line per reading on standard output
```

Both trees are compiled into `scufris-desktop` by its `build.rs`: a widget
whose TypeScript does not compile fails the build rather than the first person
who asks for it, and a backend is one embedded text rather than a file beside
the binary that can go missing or disagree with the widget naming it.

The directory name is the identifier, in both trees. See
[Widgets](../../docs/src/dev/widgets.md) for the contract, the postures, and
what a backend may do.
