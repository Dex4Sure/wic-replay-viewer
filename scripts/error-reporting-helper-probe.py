#!/usr/bin/env python3
"""Opt-in Linux desktop probe. Uses an empty profile and no replay inputs."""

import argparse
import json
import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "scenario", choices=["javascript", "native", "exit", "whole-process"]
    )
    parser.add_argument(
        "--decline", action="store_true", help="Dismiss without generating any report"
    )
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    executable = repo / "target/debug/examples/error_reporting_probe"
    artifacts = repo / "artifacts/helper-probes"
    artifacts.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix=args.scenario + "-", dir=artifacts))
    env = dict(
        os.environ, XDG_DATA_HOME=str(output), WIC_HELPER_TEST_OUTPUT=str(output)
    )
    if args.decline:
        env["WIC_HELPER_TEST_DECLINE"] = "1"
    scenario = "healthy" if args.scenario == "whole-process" else args.scenario
    application = None
    command_line = None

    def signal_application(value):
        # The supervised app is a grandchild. Verify its exact command before a signal,
        # so a process that exited and had its PID reused cannot be targeted.
        if application is not None:
            try:
                actual = Path(f"/proc/{application}/cmdline").read_bytes()
                if actual == command_line:
                    os.kill(application, value)
            except ProcessLookupError:
                pass
            except FileNotFoundError:
                pass

    with (output / "probe.log").open("w") as log:
        supervisor = subprocess.Popen(
            [str(executable), scenario], env=env, stdout=log, stderr=log
        )
        try:
            deadline = time.monotonic() + 10
            while time.monotonic() < deadline:
                children = (
                    Path(f"/proc/{supervisor.pid}/task/{supervisor.pid}/children")
                    .read_text()
                    .split()
                )
                for child in children:
                    candidate = Path(f"/proc/{child}/cmdline").read_bytes()
                    if b"--wic-application-session" in candidate:
                        application, command_line = int(child), candidate
                        break
                if application is not None:
                    break
                time.sleep(0.1)
            assert application is not None, (
                "Supervisor did not launch its application child"
            )
            if args.scenario == "whole-process":
                time.sleep(35)
                signal_application(signal.SIGSTOP)
                time.sleep(20)
                assert supervisor.poll() is None, (
                    "Supervisor stopped with its application"
                )
                reports = [
                    json.loads(p.read_text())
                    for p in (output / "wicreplayviewer/error-reports").glob("*.json")
                ]
                assert (
                    (not reports)
                    if args.decline
                    else any(r["code"] == "reportingStalled" for r in reports)
                )
                assert (output / "notification.png").is_file()
                signal_application(signal.SIGCONT)
            assert supervisor.wait(timeout=90) == 0, "Supervisor failed to exit cleanly"
        finally:
            signal_application(signal.SIGCONT)
            if supervisor.poll() is None:
                signal_application(signal.SIGTERM)
                try:
                    supervisor.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    supervisor.terminate()
                    supervisor.wait(timeout=10)
    expected = {
        "javascript": "frontendDeliveryStalled",
        "native": "nativeUiStalled",
        "exit": "unexpectedExit",
        "whole-process": "reportingStalled",
    }[args.scenario]
    reports = [
        json.loads(p.read_text())
        for p in (output / "wicreplayviewer/error-reports").glob("*.json")
    ]
    assert (
        (not reports) if args.decline else any(r["code"] == expected for r in reports)
    ), "Expected incident was not saved"
    assert (output / "notification.png").is_file(), (
        "Native notification was not rendered"
    )
    assert (output / "preview.png").is_file() != args.decline, (
        "Unexpected preview state"
    )
    if args.decline:
        assert not (output / "wicreplayviewer/error-reports").exists(), (
            "Declining created diagnostic storage"
        )
    assert not list((output / "wicreplayviewer/error-reports").glob("*.session")), (
        "Clean exit left a checkpoint"
    )
    print(
        f"Helper probe passed: {args.scenario}; native captures and reports: {output}"
    )


if __name__ == "__main__":
    main()
