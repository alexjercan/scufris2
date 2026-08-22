# Release

Use the repository [release checklist](https://github.com/alexjercan/scufris2/blob/master/RELEASE.md) for the authoritative preparation, version, verification, tag, publication, and immutability procedure. This manual does not duplicate that checklist.

Release source is consumed directly through a semantic-version tag. GitHub Release publication contains source and generated notes, not binary assets. Nix builds each selected output from the tagged source.

Documentation changes on `master` build `packages.docs` and deploy that exact output to GitHub Pages. Pull requests build the same output but do not upload or deploy it.
