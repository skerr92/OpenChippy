#!/usr/bin/env python3
"""Run the upstream GF180 deck and retain an auditable summary beside its report.

Only Ruby endless-method syntax is adapted for older KLayout embedded Ruby;
no rule geometry, tolerance, or report category is suppressed.
"""
import argparse
import collections
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("gds", type=Path)
    parser.add_argument("--deck", required=True, type=Path, help="upstream klayout/drc directory")
    parser.add_argument("--klayout", default="klayout")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    report = (args.output / "official-drc.lyrdb").resolve()
    log = args.output / "official-drc.log"
    # Never mistake a report from an earlier run for this run's output.
    if report.exists():
        parser.error(f"refusing to overwrite existing report: {report}; select a new output directory")
    with tempfile.TemporaryDirectory(prefix="openchippy-gf180-") as scratch:
        deck = Path(scratch) / "drc"
        shutil.copytree(args.deck, deck)
        adaptations = 0
        for path in deck.rglob("*.rb"):
            text = path.read_text()
            text, count = re.subn(r"^(\s*)def ([\w?!]+) = (.+)$", r"\1def \2; \3; end", text, flags=re.MULTILINE)
            if count:
                path.write_text(text)
                adaptations += count
        command = [args.klayout, "-b", "-r", str(deck / "gf180mcu.drc")]
        for option in [f"input={args.gds.resolve()}", f"report={report}", "variant=C", "run_mode=deep", "workers=1", "threads=1"]:
            command.extend(["-rd", option])
        with log.open("w") as stream:
            result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, check=False)
    if result.returncode or not report.exists():
        raise SystemExit(f"KLayout failed ({result.returncode}); see {log}")
    root = ET.parse(report).getroot()
    counts = collections.Counter(item.findtext("category", "unknown") for item in root.findall(".//items/item"))
    revision = subprocess.run(["git", "-C", str(args.deck), "rev-parse", "HEAD"], text=True, capture_output=True, check=False).stdout.strip()
    summary = {
        "gdsSha256": hashlib.sha256(args.gds.read_bytes()).hexdigest(),
        "deckCommit": revision or None,
        "variant": "C", "runMode": "deep", "rubySyntaxAdaptations": adaptations,
        "violationCount": sum(counts.values()), "byCategory": dict(sorted(counts.items())),
        "report": str(report), "ruleLogicModified": False,
    }
    (args.output / "official-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
