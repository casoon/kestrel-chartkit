#!/usr/bin/env python3
"""Regenerates the golden reference fixtures under tests/fixtures/ from the reference
implementations in kestrel_reference/.

    python3 reference/generate.py            write the fixtures
    python3 reference/generate.py --check    verify them byte for byte, write nothing
    python3 reference/generate.py --out DIR  write into DIR instead of tests/fixtures/

Requires Python 3.10 or newer and nothing else. Before generating, every module under reference/
is checked to import only the standard library; anything else aborts the run.
"""

import argparse
import ast
import difflib
import sys
from pathlib import Path

sys.dont_write_bytecode = True

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
FIXTURES = REPO / "tests" / "fixtures"
PACKAGE = "kestrel_reference"

sys.path.insert(0, str(HERE))

from kestrel_reference.fixture import render  # noqa: E402
from kestrel_reference.fixtures import ALL  # noqa: E402


def foreign_imports():
    """Every import under reference/ that is neither standard library nor this package."""
    allowed = set(sys.stdlib_module_names) | {PACKAGE}
    offenders = []
    for path in sorted(HERE.rglob("*.py")):
        tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names = [alias.name for alias in node.names]
            elif isinstance(node, ast.ImportFrom) and node.level == 0:
                names = [node.module]
            else:
                continue
            for name in names:
                if name.split(".")[0] not in allowed:
                    offenders.append(f"{path.relative_to(REPO)}: {name}")
    return offenders


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true",
                        help="compare against tests/fixtures/ instead of writing")
    parser.add_argument("--out", type=Path, default=FIXTURES,
                        help="directory to write into (default: tests/fixtures/)")
    args = parser.parse_args()

    offenders = foreign_imports()
    if offenders:
        print("reference/ may use the Python standard library only:", file=sys.stderr)
        for offender in offenders:
            print(f"  {offender}", file=sys.stderr)
        return 2

    mismatches = 0
    for module in ALL:
        text = render(*module.build())
        target = (FIXTURES if args.check else args.out) / f"{module.NAME}.txt"
        if args.check:
            current = target.read_text(encoding="utf-8") if target.exists() else ""
            if current == text:
                print(f"ok        {target.relative_to(REPO)}")
                continue
            mismatches += 1
            print(f"MISMATCH  {target.relative_to(REPO)}")
            diff = difflib.unified_diff(current.splitlines(), text.splitlines(),
                                        "committed", "generated", lineterm="", n=0)
            for line in list(diff)[:12]:
                print(f"    {line}")
        else:
            args.out.mkdir(parents=True, exist_ok=True)
            target.write_text(text, encoding="utf-8")
            print(f"wrote     {target}")
    return 1 if mismatches else 0


if __name__ == "__main__":
    sys.exit(main())
