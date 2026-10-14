import importlib.util
from pathlib import Path
import tempfile
import tomllib
import unittest


spec = importlib.util.spec_from_file_location(
    "baseline", Path(__file__).with_name("prepare-release-baseline.py")
)
baseline = importlib.util.module_from_spec(spec)
spec.loader.exec_module(baseline)


class ReleaseBaselineTests(unittest.TestCase):
    def test_old_release_uses_its_recorded_integration_versions(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest = root / "Cargo.toml"
            manifest.write_text(
                '[workspace.dependencies]\n'
                'async-openai = { version = ">=0.29.6", default-features = false }\n'
                'qdrant-client = { version = "1.15", default-features = false }\n'
                'pgvector = { version = "0.4", features = ["sqlx"] }\n'
                'spider = { version = "2.37", default-features = false }\n'
            )
            source = root / "lib.rs"
            source.write_text("pub fn old_api() {}\n")
            baseline.repair_dependencies(root, "v0.32.1")
            dependencies = tomllib.loads(manifest.read_text())["workspace"]["dependencies"]
            self.assertEqual(
                {name: dependency["version"] for name, dependency in dependencies.items()},
                {
                    "async-openai": "=0.30.1",
                    "qdrant-client": "=1.15.0",
                    "pgvector": "=0.4.1",
                    "spider": "=2.37.159",
                },
            )
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
