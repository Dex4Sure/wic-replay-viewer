"""Exercise relocated bootstrap paths without requiring private game data."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class BootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve() / "viewer"
        self.evidence = Path(self.temp.name).resolve() / "evidence"
        script = Path(__file__).with_name("bootstrap-local-data.sh")
        self.launcher = self.root / "research/scripts/bootstrap-local-data.sh"
        self.launcher.parent.mkdir(parents=True)
        shutil.copy2(script, self.launcher)
        for directory in (
            "wic",
            "replays",
            "wic_settings/Replay",
            "massgate_wicgate/bin/client",
            "massgate_wicgate/bin/server",
            "massgate_wicgate/Documents/World in Conflict/Replay",
        ):
            (self.evidence / directory).mkdir(parents=True, exist_ok=True)
        digest = hashlib.sha256(b"evidence").hexdigest()
        for name in ("wic.exe", "wic_online.exe", "wic_ds.exe"):
            (self.evidence / "wic" / name).write_bytes(b"evidence")
        manifest = self.root / "research/manifests/targets.sha256"
        manifest.parent.mkdir(parents=True)
        manifest.write_text(
            "".join(
                f"{digest}  local/binaries/game/{n}\n"
                for n in ("wic.exe", "wic_online.exe", "wic_ds.exe")
            )
        )

    def run_bootstrap(self):
        return subprocess.run(
            [str(self.launcher)],
            cwd=self.temp.name,
            env={
                **os.environ,
                "WIC_DATA_ROOT": str(self.evidence),
                "WIC_RE_ENV_FILE": str(self.root / "absent.env"),
            },
            text=True,
            capture_output=True,
            check=False,
        )

    def test_links_are_rooted_and_idempotent(self):
        for name in ("normal.wicdemo", "space name.WICDEMO", "line\nbreak.wicdemo"):
            (self.evidence / "replays" / name).write_bytes(b"replay")
        for _ in range(2):
            result = self.run_bootstrap()
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertRegex(result.stdout, r"Local evidence ready:\s+3 replay files")
        self.assertEqual(
            (self.root / "local/binaries/game").resolve(), self.evidence / "wic"
        )
        self.assertTrue((self.root / "local/generated").is_dir())
        self.assertEqual((self.evidence / "wic/wic.exe").read_bytes(), b"evidence")

    def test_missing_evidence_fails_before_creating_links(self):
        (self.evidence / "wic_settings/Replay").rmdir()
        result = self.run_bootstrap()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "local/binaries").exists())

    def test_existing_directory_and_wrong_link_are_preserved(self):
        target = self.root / "local/binaries/game"
        target.mkdir(parents=True)
        self.assertNotEqual(self.run_bootstrap().returncode, 0)
        self.assertTrue(target.is_dir())
        target.rmdir()
        target.symlink_to(self.evidence / "replays")
        self.assertNotEqual(self.run_bootstrap().returncode, 0)
        self.assertEqual(target.resolve(), self.evidence / "replays")


if __name__ == "__main__":
    unittest.main()
