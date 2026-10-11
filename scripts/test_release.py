import json
import tempfile
import unittest
from pathlib import Path
from release import TARGETS, assemble, validate


class ReleaseTests(unittest.TestCase):
    def test_versions_and_stable_tags_must_match(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "src-tauri").mkdir()
            (root / "package.json").write_text('{"version":"1.0.0"}')
            (root / "src-tauri/tauri.conf.json").write_text('{"version":"1.0.0"}')
            (root / "src-tauri/Cargo.toml").write_text('[package]\nversion="1.0.0"')
            self.assertEqual(validate(root, "v1.0.0"), "1.0.0")
            for tag in ["v1.0.1", "v1.0.0-beta", "1.0.0"]:
                with self.assertRaises(ValueError):
                    validate(root, tag)

    def test_manifest_requires_complete_signed_platforms(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "input"
            source.mkdir()
            with self.assertRaises(ValueError):
                assemble(source, root / "empty", "1.0.0", "Synthetic notes", "2026-01-01T00:00:00Z")
            for platform in TARGETS.values():
                folder = source / platform
                folder.mkdir()
                name = platform + ".bin"
                (folder / name).write_bytes(b"synthetic-artifact")
                (folder / (name + ".sig")).write_text("synthetic-signature")
                (folder / "fragment.json").write_text(json.dumps({"version": "1.0.0", "platform": platform, "name": name, "signature": "synthetic-signature"}))
            output = root / "assembled"
            assemble(source, output, "1.0.0", "Synthetic notes", "2026-01-01T00:00:00Z")
            manifest = json.loads((output / "latest.json").read_text())
            self.assertEqual(set(manifest["platforms"]), set(TARGETS.values()))
            self.assertTrue(all("/v1.0.0/" in item["url"] for item in manifest["platforms"].values()))
            fragment = next(source.rglob("fragment.json"))
            data = json.loads(fragment.read_text())
            data["signature"] = "tampered"
            fragment.write_text(json.dumps(data))
            with self.assertRaises(ValueError):
                assemble(source, root / "tampered", "1.0.0", "", "2026-01-01T00:00:00Z")


if __name__ == "__main__":
    unittest.main()
