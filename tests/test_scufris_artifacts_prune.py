import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).parents[1] / "scripts" / "scufris-artifacts-prune"


class ArtifactPruneTests(unittest.TestCase):
    def run_prune(self, root: Path) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        env["PI_CODING_AGENT_SESSION_DIR"] = str(root)
        return subprocess.run(
            [str(SCRIPT)],
            check=False,
            text=True,
            capture_output=True,
            env=env,
        )

    def sidecar(self, root: Path, name: str) -> Path:
        path = root / f"{name}.jsonl.scufris"
        path.mkdir(mode=0o700)
        artifact = path / ("a" * 24 + ".md")
        artifact.write_text("# Detail\n")
        artifact.chmod(0o600)
        metadata = path / ("a" * 24 + ".json")
        metadata.write_text("{}\n")
        metadata.chmod(0o600)
        return path

    def test_prunes_only_orphan_owned_sidecars(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            orphan = self.sidecar(root, "orphan")
            live = self.sidecar(root, "live")
            (root / "live.jsonl").write_text("{}\n")
            malformed = self.sidecar(root, "malformed")
            extra = malformed / "unexpected"
            extra.write_text("keep")
            extra.chmod(0o600)

            result = self.run_prune(root)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(orphan.exists())
            self.assertTrue(live.exists())
            self.assertTrue(malformed.exists())
            self.assertEqual(stat.S_IMODE(live.stat().st_mode), 0o700)

    def test_symlink_root_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            target = base / "target"
            target.mkdir()
            linked = base / "linked"
            linked.symlink_to(target, target_is_directory=True)
            result = self.run_prune(linked)
            self.assertEqual(result.returncode, 0)
            self.assertTrue(target.exists())


if __name__ == "__main__":
    unittest.main()
