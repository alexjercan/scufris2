import tempfile
import unittest
from pathlib import Path

from tools.release.check_versions import check_versions, collect_versions


class VersionCheckTests(unittest.TestCase):
    def fixture(
        self, root: Path, *, ios: str = "1.1.0", swift_protocol: int = 4
    ) -> None:
        files = {
            "package.json": '{"version":"1.1.0"}',
            "package-lock.json": (
                '{"version":"1.1.0","packages":{"":{"version":"1.1.0"}}}'
            ),
            "Cargo.toml": '[workspace]\n[workspace.package]\nversion = "1.1.0"\n',
            "surfaces/ios/project.yml": f"MARKETING_VERSION: {ios}\n",
            "shared/control/src/service.rs": "pub const SERVICE_VERSION: u32 = 4;\n",
            "agent/extensions/scufris/service/protocol.ts": (
                "export const SERVICE_VERSION = 4;\n"
            ),
            "surfaces/ios/Sources/Protocol.swift": (
                f"let scufrisProtocolVersion = {swift_protocol}\n"
            ),
        }
        for relative, content in files.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8")

    def test_matching_product_and_independent_protocol_versions_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root)
            product, protocol = collect_versions(root)
            self.assertEqual(set(product.values()), {"1.1.0"})
            self.assertEqual(set(protocol.values()), {"4"})
            self.assertEqual(check_versions(root, "v1.1.0"), [])

    def test_mismatched_component_protocol_and_tag_are_named(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, ios="1.0.0", swift_protocol=5)
            errors = check_versions(root, "v2.0.0")
            self.assertIn("iOS marketing is 1.0.0, expected 1.1.0", errors)
            self.assertIn("Swift surface protocol is 5, expected 4", errors)
            self.assertIn("tag is v2.0.0, expected v1.1.0", errors)


if __name__ == "__main__":
    unittest.main()
