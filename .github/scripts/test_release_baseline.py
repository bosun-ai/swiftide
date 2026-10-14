import importlib.util
from pathlib import Path
import tempfile
import unittest


spec = importlib.util.spec_from_file_location(
    "baseline", Path(__file__).with_name("prepare-release-baseline.py")
)
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class ReleaseBaselineTests(unittest.TestCase):
    def test_old_release_uses_its_recorded_openai_version(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "Cargo.toml"
            manifest.write_text(
                '[workspace.dependencies]\n'
                'async-openai = { version = ">=0.29.6", default-features = false }\n'
            )
            source = root / "lib.rs"
            source.write_text("pub fn old_api() {}\n")
            baseline.repair_dependencies(root, "v0.32.1")
            self.assertIn('version = "=0.30.1"', manifest.read_text())
            self.assertEqual(source.read_text(), "pub fn old_api() {}\n")

    def test_future_releases_need_no_dependency_repair(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "Cargo.toml"
            manifest.write_text('[workspace.package]\nversion = "0.33.0"\n')
            original = manifest.read_bytes()
            baseline.repair_dependencies(root, "v0.33.0")
            self.assertEqual(manifest.read_bytes(), original)

    def test_unexpected_old_manifest_fails_instead_of_using_a_wrong_baseline(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[workspace.dependencies]\n')
            with self.assertRaisesRegex(ValueError, "Unexpected async-openai"):
                baseline.repair_dependencies(root, "v0.32.1")


if __name__ == "__main__":
    unittest.main()
