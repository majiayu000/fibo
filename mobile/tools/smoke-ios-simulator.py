#!/usr/bin/env python3
"""Install and cold-start the Release app twice on a disposable iPhone simulator.

Screenshots require review; a live process alone is not a functional UI assertion.
No host, pairing secrets, Apple account or physical phone is used by this gate.
"""
import json
import os
from pathlib import Path
import plistlib
import subprocess
import time
import uuid

root = Path(__file__).resolve().parents[2]
app = root / "target/ios-evidence/Fibo.app"
out = root / "target/ios-evidence/native-smoke"
out.mkdir(parents=True, exist_ok=True)
os.environ.setdefault("DEVELOPER_DIR", "/Applications/Xcode.app/Contents/Developer")


def sim(*args):
    print(f"simctl {args[0]}", flush=True)
    return subprocess.check_output(["xcrun", "simctl", *args], text=True, timeout=300).strip()


with (app / "Info.plist").open("rb") as f:
    bundle = plistlib.load(f)["CFBundleIdentifier"]
runtimes = [r for r in json.loads(sim("list", "runtimes", "-j"))["runtimes"]
            if r.get("isAvailable") and ".iOS-" in r["identifier"]]
runtime = max(runtimes, key=lambda r: tuple(map(int, r["version"].split("."))))
device = sim("create", f"Fibo smoke {uuid.uuid4()}",
             "com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro", runtime["identifier"])
result = {"runtime": runtime["version"], "bundle": bundle, "starts": []}
try:
    sim("boot", device)
    sim("bootstatus", device, "-b")
    sim("install", device, str(app))
    for name in ["first-launch", "cold-restart"]:
        launched = sim("launch", device, bundle)
        pid = launched.rsplit(":", 1)[1].strip()
        time.sleep(5)
        jobs = sim("spawn", device, "launchctl", "list")
        alive = any(line.split()[0] == pid and bundle in line
                    for line in jobs.splitlines() if line.split())
        if not alive:
            raise RuntimeError(f"{name}: {bundle} exited after launch")
        sim("io", device, "screenshot", str(out / f"{name}.png"))
        result["starts"].append({"name": name, "pid": int(pid), "alive_after_seconds": 5})
        sim("terminate", device, bundle)
except Exception as error:
    result["error"] = str(error)
    # Keep the device's own install/launch failure before deleting this disposable device.
    # Host-level simctl only reports the outer IXErrorDomain error in some failures.
    try:
        log = subprocess.run([
            "xcrun", "simctl", "spawn", device, "log", "show", "--last", "3m",
            "--style", "compact", "--predicate",
            'process == "installcoordinationd" OR process == "installd" OR process == "Fibo"',
        ], capture_output=True, text=True, timeout=30)
        (out / "failure.log").write_text(log.stdout + log.stderr)
    except (OSError, subprocess.TimeoutExpired) as diagnostic_error:
        (out / "failure.log").write_text(str(diagnostic_error))
    raise
finally:
    # A broken simulator service can also hang shutdown; preserve the result and let
    # CI report the original failure instead of spending the whole job in cleanup.
    for action in ("shutdown", "delete"):
        try:
            cleanup = subprocess.run(
                ["xcrun", "simctl", action, device], capture_output=True, text=True, timeout=30)
            if cleanup.returncode:
                result.setdefault("cleanup_errors", []).append(f"{action}: {cleanup.stderr.strip()}")
        except subprocess.TimeoutExpired:
            result.setdefault("cleanup_errors", []).append(f"{action}: timed out after 30 seconds")
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
if result.get("cleanup_errors"):
    raise RuntimeError(f"Simulator cleanup failed: {result['cleanup_errors']}")
print(json.dumps(result))
