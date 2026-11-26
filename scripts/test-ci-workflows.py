#!/usr/bin/env python3
"""Replay the workflow's actual small programs with disposable data, never real secrets."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parent.parent
OPENSSL = os.environ.get("OPENSSL", "openssl")


def inline_python(workflow, step):
    text = (ROOT / ".github/workflows" / workflow).read_text().split(f"- name: {step}", 1)[1]
    text = text.split("python3 -I - <<'", 1)[1]
    marker, text = text.split("'\n", 1)
    lines = []
    for line in text.splitlines():
        if line.strip() == marker:
            break
        lines.append(line)
    return textwrap.dedent("\n".join(lines))


class WorkflowPrograms(unittest.TestCase):
    def run_python(self, source, cwd, **env):
        return subprocess.run(
            ["python3", "-I", "-c", source], cwd=cwd,
            env={**os.environ, **env}, capture_output=True, text=True,
        )

    def test_cached_retries_are_removed_even_when_first_run_passes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "target").mkdir()
            old = root / "target/ci-test-retries.txt"
            old.write_text("stale failure from yesterday\n")
            cargo = root / "cargo"
            cargo.write_text("#!/bin/sh\nexit 0\n")
            cargo.chmod(0o755)
            result = subprocess.run(
                ["sh", str(ROOT / "ci-test.sh")], cwd=root,
                env={**os.environ, "PATH": f"{root}:{os.environ['PATH']}"},
                capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(old.exists(), "an all-green run must not upload old failures")

    def test_signer_matches_rfc8032_and_rejects_an_untrusted_key(self):
        # RFC 8032 section 7.1, TEST 2. Public test data, never a publisher key.
        seed = "4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb"
        public = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"
        signature = (
            "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da"
            "085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"
        )
        source = inline_python("sign-catalog.yml", "Sign the exact index bytes with the runner's OpenSSL")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            market = root / "crates/fibo-core/src/market.rs"
            market.parent.mkdir(parents=True)
            market.write_text(f'pub const OFFICIAL_KEYS: [&str; 1] = ["{public}"];')
            catalog = root / "catalog"
            catalog.mkdir()
            (catalog / "index.json").write_bytes(b"\x72")
            # Neither a repository module nor an artifact executable may run in the signer.
            (root / "json.py").write_text('raise RuntimeError("repository code ran")')
            binary = catalog / "fibo"
            binary.write_text('#!/bin/sh\ntouch SIGNER_EXECUTED_BUILD\n')
            binary.chmod(0o755)
            bindir = root / "bin"
            bindir.mkdir()
            (bindir / "openssl").symlink_to(shutil.which(OPENSSL) or OPENSSL)
            env = dict(RUNNER_TEMP=str(root), PUBLISHER_KEY=json.dumps(dict(publisher="测试", secret=seed)),
                       PATH=f"{bindir}:{os.environ['PATH']}")
            result = self.run_python(source, root, **env)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads((catalog / "index.sig").read_text()),
                             dict(alg="ed25519", publisher="测试", key=public, sig=signature))
            self.assertEqual((catalog / "index.json").read_bytes(), b"\x72")
            self.assertFalse((root / "SIGNER_EXECUTED_BUILD").exists())
            market.write_text('pub const OFFICIAL_KEYS: [&str; 1] = ["' + "00" * 32 + '"];')
            result = self.run_python(source, root, **env)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("not one of this app's official keys", result.stderr)

    def test_publisher_refuses_any_other_staged_path(self):
        source = inline_python("sign-catalog.yml", "Publish only the signed catalog files")
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            (root / "plugins").mkdir()
            (root / "plugins/index.json").write_text("{}")
            subprocess.run(["git", "add", "plugins/index.json"], cwd=root, check=True)
            self.assertEqual(self.run_python(source, root).returncode, 0)
            (root / "unrelated").write_text("do not publish")
            subprocess.run(["git", "add", "unrelated"], cwd=root, check=True)
            result = self.run_python(source, root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("outside the catalog", result.stderr)

    def test_prose_filter_keeps_product_markdown_and_failed_diffs_in_checks(self):
        source = inline_python("ci.yml", "Skip native builds only for prose-only pull requests")
        # Substitute just the git transport: execute the unmodified selection/output logic.
        for event, paths, expected in [
            ("pull_request", ["docs/decisions/note.md", "README.md"], "false"),
            ("pull_request", ["crates/fibo-host/src/todo_skill.md"], "true"),
            ("pull_request", ["docs/site/extra.js"], "true"),
            ("pull_request", [], "true"),
            ("push", ["README.md"], "true"),
            ("workflow_dispatch", ["README.md"], "true"),
        ]:
            with self.subTest(event=event, paths=paths), tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                output = root / "out"
                injected = "import subprocess\nsubprocess.check_output = lambda _: " + repr("\0".join(paths).encode()) + "\n" + source
                result = self.run_python(injected, root, EVENT_NAME=event, GITHUB_OUTPUT=str(output))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output.read_text(), f"native={expected}\n")

        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "out"
            result = self.run_python(source, temp, EVENT_NAME="pull_request", GITHUB_OUTPUT=str(output))
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists(), "a failed git diff must never emit native=false")
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertEqual(workflow.count("if: always() && needs.changes.outputs.native != 'false'"), 2)
        self.assertIn("if: needs.changes.result != 'success'", workflow)
        self.assertIn("Workflow regressions or path selection failed", workflow)


class ActionPins(unittest.TestCase):
    # ff360fb pinned every action to a verified commit; workflows added after it went back
    # to moving tags, and nothing noticed. A tag can be moved to other code under the same name.
    def test_every_action_is_pinned_to_a_commit_with_its_version(self):
        pinned = re.compile(r"^[^@\s]+@[0-9a-f]{40} # v\S+$")
        loose = []
        for workflow in sorted((ROOT / ".github/workflows").glob("*.yml")):
            for number, line in enumerate(workflow.read_text().splitlines(), 1):
                found = re.match(r"^\s*(?:-\s+)?uses:\s*(.+?)\s*$", line)
                if not found or found[1].startswith(("./", "docker://")):
                    continue
                if not pinned.match(found[1]):
                    loose.append(f"{workflow.name}:{number}: {found[1]}")
        self.assertEqual(loose, [], "use <action>@<40-hex commit> # <version>")


if __name__ == "__main__":
    unittest.main()
