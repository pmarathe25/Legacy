#!/usr/bin/env python3
"""Run a saved study queue, verify pedigrees, and retain results as jobs finish.

Usage: python3 tools/history/study.py plan.json
Paths in the plan are relative to its `root` (the repository). Jobs run one at a
time to bound memory. This never changes simulation state or manufactures births.
"""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

import analyze


def digest(path):
    result = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(chunk)
    return result.hexdigest()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def memory_mb(pid):
    try:
        for row in Path(f"/proc/{pid}/status").read_text().splitlines():
            if row.startswith("VmRSS:"):
                return int(row.split()[1]) / 1024
    except FileNotFoundError:
        pass
    return 0


def archive_journal(manifest, destination):
    with destination.open("wb") as output:
        with gzip.GzipFile(fileobj=output, mode="wb", mtime=0, compresslevel=1) as archive:
            for segment in manifest["segments"]:
                upper = segment.get("through", float("inf"))
                with analyze.event_stream(segment, Path(".")) as stream:
                    for raw in stream:
                        try:
                            sequence = json.loads(raw)["sequence"]
                        except (json.JSONDecodeError, UnicodeDecodeError):
                            if upper != float("inf") and not raw.endswith(b"\n"):
                                break
                            raise
                        if sequence > upper:
                            break
                        if sequence > segment.get("after", 0):
                            archive.write(raw)


def publish(directory, state):
    write_json(directory / "study.json", state)
    rows = ["# Study progress", "", f"Twenty-generation target verified: **{state['verified_target']}**.", "",
            "Every completed result is checked against actual parentage and birth/death journals.", "",
            "| Run | State | Days | Depth | Births | Living mortals |",
            "| --- | --- | ---: | ---: | ---: | ---: |"]
    for result in state["runs"]:
        rows.append("| " + " | ".join(str(result.get(k, "")) for k in
                    ["name", "state", "days", "depth", "births", "mortals"]) + " |")
    rows += ["", "Extinction is an outcome. Memory limits and external stops are censoring, not biological deaths.",
             "Pedigree strata overlap in time. Trait/reproductive associations do not isolate selection from drift,",
             "mutation, family preferences, or culture. Young surviving cohorts remain censored.", ""]
    (directory / "progress.md").write_text("\n".join(rows))


def run(plan_path):
    plan = json.loads(Path(plan_path).read_text())
    root = Path(plan["root"]).resolve()
    executable = root / plan["executable"]
    if digest(executable) != plan["binary_sha256"]:
        raise ValueError("study executable differs from the frozen binary")
    directory = root / plan["artifacts"]
    directory.mkdir(parents=True, exist_ok=True)
    target = int(plan.get("generations", 20))
    if target < 20:
        raise ValueError("this study runner requires at least twenty generations")
    state = {"binary_sha256": plan["binary_sha256"], "generations": target,
             "verified_target": False, "runs": []}
    for job in plan["jobs"]:
        result = {"name": job["name"], "state": "queued"}
        state["runs"].append(result)
    publish(directory, state)
    for job, result in zip(plan["jobs"], state["runs"]):
        if state["verified_target"] and plan.get("stop_after_target", False):
            result["state"] = "not started after verified target"
            publish(directory, state)
            continue
        output = root / job["output"]
        if output.exists():
            raise ValueError(f"refusing to overwrite a study directory: {output}")
        output.mkdir(parents=True)
        stop = output / "stop-request"
        args = [str(executable), "--days", str(plan.get("days", 365000)),
                "--generations", str(target), "--sample-days", "3650",
                "--checkpoint-days", "3650", "--journal", "notable",
                "--output", str(output), "--stop-file", str(stop)]
        segments = list(job.get("segments", []))
        segments = [{**s, "directory": str(root / s["directory"])} for s in segments]
        if job.get("resume"):
            args += ["--resume", str(root / job["resume"])]
        else:
            args += ["--seed", str(job["seed"])]
            for overlay in job.get("mods", []):
                args += ["--mod", str(root / overlay)]
        result["state"] = "running"
        result["output"] = str(output)
        publish(directory, state)
        print(f"starting {job['name']}", flush=True)
        with (output / "process.log").open("w") as log:
            process = subprocess.Popen(args, cwd=root, stdout=log, stderr=log)
            while process.poll() is None:
                rss = memory_mb(process.pid)
                result["rss_mb"] = round(rss, 1)
                if rss > plan.get("memory_mb", 2400) and not stop.exists():
                    stop.write_text("study memory limit; finish at a committed day\n")
                    result["censoring"] = "memory limit"
                if (directory / "stop-request").exists() and not stop.exists():
                    stop.write_text("external study stop; finish at a committed day\n")
                    result["censoring"] = "external stop"
                daily = output / "daily.jsonl"
                if daily.exists():
                    with daily.open("rb") as stream:
                        stream.seek(max(0, daily.stat().st_size - 100_000))
                        for raw in reversed(stream.read().splitlines()):
                            try:
                                sample = json.loads(raw)
                            except json.JSONDecodeError:
                                continue
                            result.update(days=sample["day"], depth=sample["generations"],
                                          births=sample["births"], mortals=sample["mortals"])
                            break
                publish(directory, state)
                time.sleep(5)
        result["exit_code"] = process.returncode
        if process.returncode != 0:
            result["state"] = "execution failed; not a biological outcome"
            publish(directory, state)
            continue
        summary = json.loads((output / "summary.json").read_text())
        result.update(days=summary["days"], depth=summary["final"]["generations"],
                      births=summary["final"]["births"], mortals=summary["final"]["mortals"])
        result["state"] = "extinct" if summary["extinct"] else "censored" if summary["stopped"] else "completed"
        after = segments[-1]["through"] if segments else 0
        segments.append({"directory": str(output), "after": after})
        manifest = {"final_directory": str(output), "segments": segments}
        artifact = directory / job["name"]
        artifact.mkdir(exist_ok=True)
        write_json(artifact / "segments.json", manifest)
        analyze.analyze(artifact / "segments.json", artifact)
        archive_journal(manifest, artifact / "events.jsonl.gz")
        result["verified_depth"] = result["depth"]
        result["hashes"] = {name: digest(artifact / name) for name in
                            ["analysis.json", "people.csv", "events.jsonl.gz", "segments.json"]}
        state["verified_target"] |= result["depth"] >= target
        publish(directory, state)
        print(f"verified {job['name']}: depth {result['depth']}, {result['state']}", flush=True)
        if (directory / "stop-request").exists():
            break
    state["finished"] = True
    publish(directory, state)


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    run(sys.argv[1])
