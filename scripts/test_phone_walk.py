"""python3 -I scripts/test_phone_walk.py — no phone, no host: fibo and adb are faked."""
import contextlib
import importlib.util
import io
import json
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("phone_walk", Path(__file__).with_name("phone-walk.py"))
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)


class StoppedRun(unittest.TestCase):
    def setUp(self):
        self.pages = Path(tempfile.mkdtemp())
        self.run = self.pages / "2026-11-18-phone-walk-2"
        self.run.mkdir()
        self.test_dir = Path(tempfile.mkdtemp(prefix="phone-walk-"))
        (self.run / "results.json").write_text(json.dumps({
            "items": {}, "tests": [{"dir": str(self.test_dir), "terminal": "t-claude"},
                                   {"dir": str(self.test_dir), "terminal": "t-codex", "codex": True}]}))
        self.lock = self.pages / ".phone-walk-lock"
        self.lock.write_text(f"someone-else\n{self.run}\n")
        self.closed = []
        home = Path(tempfile.mkdtemp())
        patches = [patch.object(walk.Path, "home", classmethod(lambda cls: home)),patch.object(walk, "LOCK", self.lock), patch.object(walk, "PAGES", self.pages),
                   patch.object(walk, "fibo", lambda: "fibo"),
                   patch.object(walk, "forget_codex_trust", lambda dirs: None),
                   patch.object(walk.subprocess, "run", self.fake_run)]
        for p in patches:
            p.start()
            self.addCleanup(p.stop)

    def fake_run(self, cmd, **kw):
        if cmd[:3] == ["fibo", "list", "--json"]:
            out = json.dumps([{"id": "t-claude", "cwd": str(self.test_dir)},
                              {"id": "t-codex", "cwd": str(self.test_dir)},
                              {"id": "mine", "cwd": "/Users/me/code"}])
            return subprocess.CompletedProcess(cmd, 0, out, "")
        if cmd[:2] == ["fibo", "close"]:
            self.closed.append(cmd[2])
        return subprocess.CompletedProcess(cmd, 0, "", "")

    def test_the_lock_names_a_run_that_never_tore_down(self):
        self.assertEqual(walk.unfinished(), str(self.run))
        self.lock.unlink()
        self.assertIsNone(walk.unfinished())

    def test_tearing_down_a_stopped_run_closes_only_its_terminals_once(self):
        with contextlib.redirect_stdout(io.StringIO()) as out:
            walk.teardown(str(self.run))
        self.assertEqual(sorted(self.closed), ["t-claude", "t-codex"])
        self.assertFalse(self.test_dir.exists())
        self.assertIn("删了 1 个测试目录", out.getvalue(), "both sessions share one dir")
        self.assertFalse(self.lock.exists(), "the lock named this run, so it goes with it")
        self.assertIn("torn_down", json.loads((self.run / "results.json").read_text()))
        walk.teardown(str(self.run))
        self.assertEqual(len(self.closed), 2, "a second teardown closes nothing")

    def test_a_lock_on_another_run_stays(self):
        other = self.pages / "other-run"
        other.mkdir()
        self.lock.write_text(f"someone-else\n{other}\n")
        walk.teardown(str(self.run))
        self.assertTrue(self.lock.exists())


if __name__ == "__main__":
    unittest.main()
