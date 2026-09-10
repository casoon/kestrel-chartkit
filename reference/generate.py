#!/usr/bin/env python3
"""Regenerates the golden reference fixtures under tests/fixtures/ from the reference
implementations in kestrel_reference/.

    python3 reference/generate.py            write the fixtures
    python3 reference/generate.py --check    verify them byte for byte, write nothing
    python3 reference/generate.py --out DIR  write into DIR instead of tests/fixtures/

Requires Python 3.10 or newer and nothing else. Before generating, every module under reference/
is checked to import only the standard library; anything else aborts the run.

Some fixtures are only partly derived so far. Their modules set `PARTIAL = True` and return the
keys they derive from `derive()`. Such a fixture is never written; both modes compare the derived
keys against the committed file and report how much of it is covered. A fixture becomes fully
generated once every one of its keys is derived.
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

from kestrel_reference.fixture import parse_values, render  # noqa: E402
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


def check_partial(module):
    """Compares the keys a partial module derives with the committed fixture.

    Returns the number of derived keys, the number of committed keys and a list of problems. A
    derived key must exist in the committed file and agree with it to 1e-12 relative (or absolute
    below 1); a module may set its own `TOLERANCE`.
    """
    target = FIXTURES / f"{module.NAME}.txt"
    committed = parse_values(target.read_text(encoding="utf-8"))
    derived = module.derive()
    tolerance = getattr(module, "TOLERANCE", 1e-12)
    # Keys whose committed value was produced under a documented, different reading of the same
    # definition: accepted up to their stated bound, reported, and replaced once the fixture is
    # fully generated.
    differences = getattr(module, "DIFFERENCES", {})
    problems, explained = [], 0
    for key, value in derived.items():
        if key not in committed:
            problems.append(f"{key}: not in the committed fixture")
            continue
        scale = max(1.0, abs(committed[key]))
        deviation = abs(value - committed[key])
        if deviation <= tolerance * scale:
            continue
        if key in differences and deviation <= differences[key][0] * scale:
            explained += 1
            continue
        problems.append(f"{key}: derived {value!r}, committed {committed[key]!r}")
    # Stated constants (comparison tolerances the tests apply) are not derivations; they are
    # listed separately and must match exactly.
    constants = getattr(module, "CONSTANTS", {})
    for key, value in constants.items():
        if committed.get(key) != value:
            problems.append(f"{key}: stated {value!r}, committed {committed.get(key)!r}")
    return len(derived), explained, len(constants), len(committed), problems


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
        if getattr(module, "PARTIAL", False):
            derived, explained, stated, total, problems = check_partial(module)
            name = (FIXTURES / f"{module.NAME}.txt").relative_to(REPO)
            if problems:
                mismatches += 1
                print(f"MISMATCH  {name} (partial)")
                for problem in problems[:12]:
                    print(f"    {problem}")
            else:
                note = f" ({explained} under a documented different reading)" if explained else ""
                print(f"partial   {name}: {derived} derived{note} + {stated} stated of {total} "
                      "keys, all agree; not written")
            continue
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
