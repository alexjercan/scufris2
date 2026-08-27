# The helpers that are neither Rust nor TypeScript. `npm test` globs
# `tests/*.test.ts` and `cargo test` knows nothing outside `native/`, so the
# Python beside them - the job runner, the artifact prune, the quick-review
# agent, and the subscription backends - was covered by tests that no gate ran.
# This is the gate.
{
  self,
  pkgs,
  ...
}: {
  helper-tests =
    pkgs.runCommand "scufris-helper-tests" {
      # `git` and `tmux`, because the job runner's tests build a repository,
      # commit in it, and spawn a worker in a pane. The runner reports either
      # one as a missing executable rather than assuming it, so a machine
      # without them gets a clear refusal - which is why they are named here
      # and not in the package. `util-linux` is for `flock`, which the staging
      # tests skip without, and a check that skips is a check that says nothing.
      nativeBuildInputs = [pkgs.python3 pkgs.git pkgs.tmux pkgs.util-linux];
    } ''
      # Copied rather than run in place: every test resolves what it exercises
      # relative to its own file, and the store is read-only while some of them
      # write beside what they read.
      cp -r ${self} source
      chmod -R u+w source
      cd source

      # Every helper starts `#!/usr/bin/env python3`, and the sandbox has no
      # `/usr/bin/env`. A missing interpreter is reported as a missing program,
      # so without this the tests fail saying the helper is not there.
      patchShebangs .
      # The fake agents are written into a scratch directory during the run,
      # which is past every patch, so the shebang is rewritten in the source
      # the tests write from. This is the one way the check differs from
      # running the suite by hand.
      sed -i "s|#!/usr/bin/env python3|#!$(command -v python3)|g" tests/*.py

      # A socket path is limited to 108 bytes and the sandbox build directory
      # is deep enough for that to matter.
      export TMPDIR=/tmp
      python3 -m unittest discover -s tests -p 'test_*.py'
      touch "$out"
    '';
}
