"""Run against a built host: python3 -I scripts/test_perf_fixture.py BIN_DIR.

No window or model is opened. Every assertion checks actual fixture processes.
"""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


fixture = load('perf_fixture', 'perf-fixture.py')
gate = load('perf_gate', 'perf-gate.py')
BINDIR = Path(sys.argv.pop(1)).resolve()


class CleanupTests(unittest.TestCase):
    def setUp(self):
        # Keep Unix socket names below the macOS limit, even in a long worktree path.
        self.temp = tempfile.TemporaryDirectory(prefix='perf-clean-', dir='/tmp')
        self.base = Path(self.temp.name)
        self.roots = []
        self.children = []
        self.real_popen = subprocess.Popen
        self.saved = {}

    def tearDown(self):
        for root in self.roots:
            fixture.stop(root)
        for child in self.children:
            if child.poll() is None:
                fixture.stop_tree(child.pid)
            child.wait(timeout=10)
        self.temp.cleanup()

    def root(self):
        root = self.base / str(len(self.roots))
        self.roots.append(root)
        return root

    def popen(self, *args, **kwargs):
        child = self.real_popen(*args, **kwargs)
        if kwargs.get('start_new_session'):
            self.children.append(child)
        return child

    def prepare(self, root):
        with patch.object(fixture.subprocess, 'Popen', self.popen), contextlib.redirect_stdout(io.StringIO()):
            fixture.prepare(root, BINDIR, 8, 4)
        state = json.loads((root / 'state.json').read_text())
        self.saved = {int(pid): row for pid, row in state['processes'].items()}
        return state

    def assert_gone(self, saved):
        current = fixture.processes()
        alive = [pid for pid, row in saved.items()
                 if pid in current and current[pid][3] == row[3] and 'Z' not in current[pid][2]]
        self.assertEqual(alive, [], f'fixture processes still alive: {alive}')

    def test_normal_stop_reaps_shells_and_sleep_children(self):
        root = self.root()
        self.prepare(root)
        self.assertGreaterEqual(len(self.saved), 5)
        fixture.stop(root)
        self.assert_gone(self.saved)
        self.assertFalse((root / 'state.json').exists())
        fixture.stop(root)  # a caller's finally can repeat cleanup

    def test_cli_prepare_and_stop_use_the_explicit_run_directory(self):
        root = self.root()
        command = [sys.executable, '-I', str(Path(__file__).with_name('perf-fixture.py'))]
        subprocess.run(command + ['prepare', str(root), str(BINDIR), '8', '4'],
                       check=True, capture_output=True, timeout=30)
        state = json.loads((root / 'state.json').read_text())
        saved = {int(pid): row for pid, row in state['processes'].items()}
        subprocess.run(command + ['stop', str(root)], check=True, timeout=15)
        self.assert_gone(saved)

    def test_two_preparations_do_not_overwrite_or_stop_each_other(self):
        first, second = self.root(), self.root()
        legacy = self.base / 'Library/Caches/MooringPerf/state.json'
        legacy.parent.mkdir(parents=True)
        legacy.write_text(json.dumps({'host': os.getpid()}))
        with patch.dict(os.environ, {'HOME': str(self.base)}):
            self.prepare(first)
            saved_first = self.saved
            self.prepare(second)
            saved_second = self.saved
        fixture.stop(first)
        self.assert_gone(saved_first)
        terminals = fixture.rpc(second, 'Workspace')['Workspace']['terminals']
        self.assertEqual(len(terminals), 4)
        current = fixture.processes()
        self.assertTrue(all(t['pid'] in current and 'Z' not in current[t['pid']][2] for t in terminals))
        fixture.stop(second)
        self.assert_gone(saved_second)
        self.assertEqual(json.loads(legacy.read_text()), {'host': os.getpid()})

    def fail_during_launch(self, error):
        root = self.root()
        original = fixture.rpc
        saved = {}

        def fail(root, request):
            reply = original(root, request)
            if isinstance(request, dict) and request.get('Launch', {}).get('id') == 'p1':
                state = json.loads((root / 'state.json').read_text())
                saved.update(fixture.owned_processes(fixture.processes(), [state['host']]))
                error()
            return reply

        with patch.object(fixture, 'rpc', fail), patch.object(fixture.subprocess, 'Popen', self.popen):
            with self.assertRaises((RuntimeError, KeyboardInterrupt)):
                fixture.prepare(root, BINDIR, 8, 4)
        self.assertGreaterEqual(len(saved), 3)
        self.assert_gone(saved)
        self.assertFalse((root / 'state.json').exists())

    def test_failed_prepare_cleans_the_already_launched_terminals(self):
        def fail():
            raise RuntimeError('injected failure after the second terminal')
        self.fail_during_launch(fail)

    def test_sigterm_during_prepare_cleans_the_already_launched_terminals(self):
        old = signal.signal(signal.SIGTERM, fixture.interrupted)
        try:
            self.fail_during_launch(lambda: os.kill(os.getpid(), signal.SIGTERM))
        finally:
            signal.signal(signal.SIGTERM, old)

    def test_stop_after_host_exit_still_reaps_surviving_terminals(self):
        root = self.root()
        original = fixture.rpc

        def survive_hangup(root, request):
            if isinstance(request, dict) and 'Launch' in request:
                request['Launch']['argv'][2] = "trap '' HUP; while :; do sleep 3600; done"
            return original(root, request)

        with patch.object(fixture, 'rpc', survive_hangup):
            state = self.prepare(root)
        os.kill(state['host'], signal.SIGTERM)
        self.children[-1].wait(timeout=10)
        current = fixture.processes()
        self.assertTrue(any(p != state['host'] and p in current and 'Z' not in current[p][2]
                            for p in self.saved), 'case must leave live children after the host exits')
        fixture.stop(root)
        self.assert_gone(self.saved)

    def test_stop_does_not_signal_a_pid_with_a_different_start_time(self):
        root = self.root()
        self.prepare(root)
        state_file = root / 'state.json'
        state = json.loads(state_file.read_text())
        original = state_file.read_text()
        state['processes'] = {str(os.getpid()): [os.getppid(), os.getpgrp(), 'S', 'not this process']}
        state_file.write_text(json.dumps(state))
        try:
            with patch.object(fixture.os, 'killpg', side_effect=AssertionError('signalled a reused PID')):
                fixture.stop(root)
        finally:
            state_file.write_text(original)
        fixture.stop(root)
        self.assert_gone(self.saved)

    def gate_failure(self, phase):
        original_run = subprocess.run
        saved = {}

        def run(args, **kwargs):
            if args[:len(gate.FIXTURE)] != gate.FIXTURE:
                return original_run(args, **kwargs)
            command, root = args[len(gate.FIXTURE):len(gate.FIXTURE) + 2]
            root = Path(root)
            if root not in self.roots:
                self.roots.append(root)
            if command == 'prepare':
                self.prepare(root)
                saved.update(self.saved)
                if phase == 'prepare':
                    raise subprocess.CalledProcessError(1, args)
            elif command == 'probe':
                raise subprocess.TimeoutExpired(args, 600)
            elif command == 'stop':
                fixture.stop(root)
            return subprocess.CompletedProcess(args, 0, stdout='')

        with patch.object(gate, 'LOCK', self.base / 'gate.lock'), patch.object(gate.subprocess, 'run', run):
            with self.assertRaises((subprocess.CalledProcessError, subprocess.TimeoutExpired)):
                gate.probe(BINDIR, self.base / 'out', 'frames-pane', False)
        self.assertGreaterEqual(len(saved), 5)
        self.assert_gone(saved)

    def test_gate_cleans_up_when_prepare_reports_failure(self):
        self.gate_failure('prepare')

    def test_gate_cleans_up_when_probe_times_out(self):
        self.gate_failure('probe')

    def test_stop_tree_tolerates_group_becoming_zombie_after_snapshot(self):
        dying = self.popen(['/bin/sh', '-c', 'sleep 0.15'], start_new_session=True)
        staying = self.popen(['/bin/sh', '-c', 'sleep 3600'], start_new_session=True)
        initial = fixture.processes()
        recorded = {p: initial[p] for p in (dying.pid, staying.pid)}
        calls = 0
        real_processes = fixture.processes

        def snapshot_then_zombie():
            nonlocal calls
            calls += 1
            table = real_processes()
            if calls == 2:
                # Let `dying` finish into an unreaped zombie before killpg runs on its group.
                for _ in range(30):
                    if 'Z' in real_processes().get(dying.pid, ('', '', 'Z'))[2]:
                        break
                    fixture.time.sleep(0.02)
            return table

        with patch.object(fixture, 'processes', snapshot_then_zombie):
            fixture.stop_tree(staying.pid, recorded)
        self.assert_gone(recorded)


if __name__ == '__main__':
    unittest.main()
