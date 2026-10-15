#!/usr/bin/env python3
"""Run release fixtures and verify canonical hashes across workers and repeats."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--workload", choices=["all", "economy", "latency", "storage", "ledger", "perception", "traits", "architecture"], default="all")
parser.add_argument("--population", nargs="+", type=int, default=[15, 1000, 10000, 20000])
parser.add_argument("--workers", nargs="+", type=int, default=sorted({1, 2, 4, min(8, os.cpu_count() or 1)}))
parser.add_argument("--repeats", type=int, default=3)
parser.add_argument("--allocation-profile", action="store_true")
parser.add_argument("--system-allocator", action="store_true")
args = parser.parse_args()
if args.repeats < 1:
    parser.error("repeats must be positive")
root = Path(__file__).resolve().parents[2]
command = ["cargo", "bench", "-p", "game-performance", "--no-run", "--message-format=json", "--locked", "--offline", "-j", "2"]
if args.system_allocator:
    command.append("--no-default-features")
if args.allocation_profile:
    command.extend(["--features", "allocation-profile"])
build = subprocess.run(command, cwd=root, capture_output=True, text=True)
if build.returncode:
    raise SystemExit(build.stderr)
binaries = {}
for line in build.stdout.splitlines():
    artifact = json.loads(line)
    if artifact.get("reason") == "compiler-artifact" and artifact.get("executable") and "bench" in artifact["target"]["kind"]:
        binaries[artifact["target"]["name"]] = artifact["executable"]


def run(name, environment):
    result = subprocess.run([binaries[name]], cwd=root, env=os.environ | environment, capture_output=True, text=True)
    print(result.stdout, end="", flush=True)
    if result.returncode:
        raise SystemExit(result.stderr)
    return result.stdout


canonical = {}
if args.workload in ["all", "economy"]:
    for population in args.population:
        for distributed in [False, True]:
            for workers in args.workers:
                for repeat in range(args.repeats):
                    print(f"workload=economy repeat={repeat} distributed={distributed}", flush=True)
                    output = run("economy", {"LEGACY_POPULATION": str(population), "LEGACY_WORKERS": str(workers), "LEGACY_DISTRIBUTED": str(distributed).lower()})
                    state = re.search(r"hash=([0-9a-f]+)", output)[1]
                    key = (population, distributed)
                    if key in canonical and canonical[key] != state:
                        raise SystemExit(f"canonical state differs for {key}, workers={workers}, repeat={repeat}")
                    canonical[key] = state
if args.workload in ["all", "latency"]:
    for population in args.population:
        for workers in args.workers:
            for repeat in range(args.repeats):
                print(f"workload=latency repeat={repeat}", flush=True)
                run("latency", {"LEGACY_POPULATION": str(population), "LEGACY_WORKERS": str(workers)})
for name in ["storage", "ledger", "traits", "architecture"]:
    if args.workload in ["all", name]:
        run(name, {})

if args.workload in ["all", "perception"]:
    for memories, patches, roads, local in [(0, 1, 0, 1), (1000, 1, 1000, 1), (1000, 10000, 1000, 1), (0, 64, 0, 64), (1000, 64, 1000, 64)]:
        for workers in args.workers:
            for repeat in range(args.repeats):
                print(f"workload=perception repeat={repeat} memories={memories} patches={patches} local_patches={local}", flush=True)
                output = run("perception", {"LEGACY_POPULATION": "1000", "LEGACY_WORKERS": str(workers), "LEGACY_MEMORIES": str(memories), "LEGACY_PATCHES": str(patches), "LEGACY_ROADS": str(roads), "LEGACY_LOCAL_PATCHES": str(local)})
                state = re.search(r"hash=([0-9a-f]+)", output)[1]
                key = ("perception", memories, patches, roads, local)
                if key in canonical and canonical[key] != state:
                    raise SystemExit(f"perception canonical state differs for {key}, workers={workers}")
                canonical[key] = state
