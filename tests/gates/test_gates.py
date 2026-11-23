import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("gates", Path(__file__).resolve().parents[2] / "scripts/gates.py")
gates = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gates)
ROOT = Path(__file__).resolve().parents[2]


class Router(unittest.TestCase):
    def test_frontend_is_independent(self):
        plan = gates.route(ROOT, ["web/src/features/jobs/JobQueue.vue"])
        self.assertTrue(plan["gates"]["frontend"])
        self.assertTrue(plan["gates"]["integration"])
        self.assertFalse(plan["gates"]["backend"])
        self.assertFalse(plan["gates"]["environment"])
        self.assertIn("web/tests/e2e/jobs.spec.ts", plan["specs"])
        self.assertNotIn("web/tests/e2e/scan-stack.spec.ts", plan["specs"])

    def test_backend_does_not_request_node_gate(self):
        plan = gates.route(ROOT, ["crates/oscope-api/src/auth.rs"])
        self.assertTrue(plan["gates"]["backend"])
        self.assertTrue(plan["gates"]["integration"])
        self.assertFalse(plan["gates"]["frontend"])
        self.assertIn("oscope-server", plan["packages"])
        self.assertNotIn("oscope-core", plan["packages"])

    def test_docs_only_and_test_only(self):
        self.assertEqual([n for n, on in gates.route(ROOT, ["docs/phases/r08.md"])["gates"].items() if on], ["docs"])
        self.assertEqual([n for n, on in gates.route(ROOT, ["web/src/api/client.test.ts"])["gates"].items() if on], ["frontend"])

    def test_hardware_tooling_does_not_select_application_or_image_builds(self):
        for name in ["scripts/hardware-host.ps1", "scripts/hardware-linux.py", "tests/hardware/test_inventory.py",
                     "scripts/check-hardware.sh", "configs/hardware.example.toml"]:
            with self.subTest(name=name):
                self.assertEqual([n for n, on in gates.route(ROOT, [name])["gates"].items() if on], ["hardware"])
                self.assertFalse(gates.backend_input(name))

    def test_shared_inputs_and_deleted_unknown_feature(self):
        for name in ["Cargo.lock", "scripts/gates.py", "web/package-lock.json", "python/oscope-ext/README.md"]:
            self.assertTrue(gates.route(ROOT, [name])["gates"]["integration"])
        self.assertTrue(gates.route(ROOT, ["scripts/gates.py"])["gates"]["frontend"])
        self.assertTrue(gates.route(ROOT, ["web/src/features/gone/Old.vue"])["specs"])

    def test_backend_cache_ignores_frontend_but_tracks_sdk_and_local_backend(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root).decode().strip()
            git("init", "-q")
            git("config", "user.email", "gate-test@example.invalid")
            git("config", "user.name", "Gate test")
            for name in ["web", "python", "crates"]:
                (root / name).mkdir()
            (root / "web/app.vue").write_text("original")
            (root / "python/sdk.py").write_text("original")
            (root / "crates/lib.rs").write_text("original")
            git("add", ".")
            git("commit", "-qm", "backend")
            revision = git("rev-parse", "HEAD")
            key = gates.backend_key(root)
            (root / "web/app.vue").write_text("changed")
            (root / "README.md").write_text("new docs")
            self.assertEqual(gates.backend_key(root), key)
            self.assertEqual(gates.backend_revision(root), revision)
            (root / "python/sdk.py").write_text("changed")
            self.assertNotEqual(gates.backend_key(root), key)
            self.assertEqual(gates.backend_revision(root), "unknown")
            (root / "python/sdk.py").write_text("original")
            (root / "python/oscope-ext").mkdir()
            (root / "python/oscope-ext/README.md").write_text("SDK packaging input")
            self.assertNotEqual(gates.backend_key(root), key)
            (root / "python/oscope-ext/README.md").unlink()
            (root / "crates/new.rs").write_text("new backend input")
            self.assertNotEqual(gates.backend_key(root), key)
            (root / "crates/new.rs").unlink()
            git("add", "web/app.vue", "README.md")
            git("commit", "-qm", "frontend and documentation")
            self.assertEqual(gates.backend_revision(root), revision)
            git("rm", "python/sdk.py")
            git("commit", "-qm", "remove obsolete backend input")
            self.assertEqual(gates.backend_revision(root), git("rev-parse", "HEAD"))

    def test_git_union_including_both_rename_sides(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root).decode().strip()
            def write(name):
                (root / name).write_text(name)
            git("init", "-q")
            git("config", "user.email", "gate-test@example.invalid")
            git("config", "user.name", "Gate test")
            for name in ["committed", "staged", "worktree", "deleted", "rename"]:
                write(name)
            git("add", ".")
            git("commit", "-qm", "base")
            base = git("rev-parse", "HEAD")
            write("commit-new")
            git("add", "commit-new")
            git("commit", "-qm", "head")
            (root / "staged").write_text("changed")
            git("add", "staged")
            (root / "worktree").write_text("changed")
            (root / "deleted").unlink()
            git("mv", "rename", "renamed")
            write("untracked")
            self.assertEqual(set(gates.changed_files(root, base)), {
                "commit-new", "staged", "worktree", "deleted", "rename", "renamed", "untracked"})
            self.assertEqual(gates.changed_files(root, base, local=False), ["commit-new"])


if __name__ == "__main__":
    unittest.main()
