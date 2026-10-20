#!/usr/bin/env python3
"""Exercise repository-owned alpha fixtures; retain native evidence and hashes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(qualifier, output):
    # A fresh directory prevents stale evidence from satisfying a failed run.
    output.mkdir(parents=True, exist_ok=False)
    results = []
    for process, deck in [
        ("edu", "openchippy-edu-5m.yaml"),
        ("gf180", "process_gf180mcu_3v3_5m_dr.yaml"),
    ]:
        technology = ROOT / "docs/examples" / deck
        for circuit, devices in [("inverter", 2), ("nand2", 4)]:
            source = ROOT / "docs/examples/qualification" / f"{circuit}.chippy"
            case = output / f"{process}-{circuit}"
            for attempt in ("first", "repeat"):
                subprocess.run([
                    str(qualifier), str(source), str(technology),
                    str(case / attempt), "--strict",
                ], check=True, cwd=ROOT)
            first = case / "first"
            report = json.loads((first / "native-report.json").read_text())
            if not report["nativeChecksPassed"]:
                raise RuntimeError(f"{case.name}: qualifier returned success with failed checks")
            if report["lvs"]["expectedDeviceCount"] != devices:
                raise RuntimeError(f"{case.name}: fixture device inventory changed")
            if digest(first / "layout.gds") != digest(case / "repeat/layout.gds"):
                raise RuntimeError(f"{case.name}: fresh generations produced different GDS")
            results.append({
                "case": case.name,
                "sourceSha256": digest(source),
                "technologyYamlSha256": digest(technology),
                "technologyFingerprint": report["technologyFingerprint"],
                "physicalIrVersion": report["physicalIrVersion"],
                "gdsSha256": digest(first / "layout.gds"),
                "nativeChecksPassed": report["nativeChecksPassed"],
            })

    # Prove the runner rejects actual geometry failures, rather than trusting
    # only successful process exit codes. Keep the failed evidence for review.
    broken = json.loads((output / "edu-inverter/first/layout.json").read_text())
    metal = next(s for s in broken["shapes"] if s["layer"] == "metal1")
    metal["width"] = 0.001
    broken_path = output / "invalid-layout.json"
    broken_path.write_text(json.dumps(broken))
    failure = subprocess.run([
        str(qualifier), str(broken_path),
        str(ROOT / "docs/examples/openchippy-edu-5m.yaml"),
        str(output / "expected-failure"), "--strict",
    ], check=False, cwd=ROOT)
    evidence = json.loads((output / "expected-failure/native-report.json").read_text())
    if failure.returncode == 0 or evidence["drc"]["errorCount"] == 0 or evidence["nativeChecksPassed"]:
        raise RuntimeError("strict qualification accepted intentionally invalid geometry")
    mismatched = json.loads((output / "edu-inverter/first/layout.json").read_text())
    mismatched["technologyFingerprint"] = "unqualified-process"
    mismatch_path = output / "mismatched-layout.json"
    mismatch_path.write_text(json.dumps(mismatched))
    mismatch = subprocess.run([
        str(qualifier), str(mismatch_path),
        str(ROOT / "docs/examples/openchippy-edu-5m.yaml"),
        str(output / "expected-fingerprint-failure"), "--strict",
    ], check=False, cwd=ROOT)
    evidence = json.loads((output / "expected-fingerprint-failure/native-report.json").read_text())
    if mismatch.returncode == 0 or evidence["nativeChecksPassed"] or evidence["drc"]["errorCount"] != 0:
        raise RuntimeError("strict qualification failed to isolate a process fingerprint mismatch")
    summary = {
        "cases": results, "invalidGeometryRejected": True,
        "processMismatchRejected": True, "officialQualification": "NOT_RUN",
    }
    (output / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"Passed {len(results)} native fixture cases, deterministic regeneration, and failure rejection.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--qualifier", type=Path, default=ROOT / "src-tauri/target/debug/examples/qualify_physical")
    parser.add_argument("--output", type=Path, required=True, help="new evidence directory")
    args = parser.parse_args()
    check(args.qualifier.resolve(), args.output.resolve())


if __name__ == "__main__":
    main()
