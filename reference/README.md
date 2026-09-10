# Reference generator

The golden fixtures under `tests/fixtures/` hold the values the Rust implementation is checked
against. This directory holds the code those values come from, so that every one of them can be
reproduced by anyone with a Python interpreter.

```bash
python3 reference/generate.py            # write the fixtures
python3 reference/generate.py --check    # verify them byte for byte
```

Python 3.10 or newer; nothing else.

## Rules

- **Written from the documented formula, not from the Rust code.** A second implementation is
  only independent if it did not start as a translation of the first. Where a formula leaves a
  convention open (seed, divisor, extrapolation), the crate's documentation decides, and every
  such choice is stated in the fixture header.
- **Python standard library only.** `generate.py` parses every module here and refuses to run if
  anything else is imported.
- **Deterministic.** The same code produces the same bytes; `--check` is part of the review chain.
- **Documented alternatives are generated, not asserted.** Where the crate deliberately follows
  one of several established conventions, the fixture carries the value under the other one as
  well, marked as such, so the choice is visible as a number.

## Coverage

| Fixture | Module |
|---|---|
| `golden_option_diff` | `kestrel_reference/fixtures/option_diff.py` |
| `golden_bond_diff` | `kestrel_reference/fixtures/bond_diff.py` |
| `golden_curve_diff` | `kestrel_reference/fixtures/curve_diff.py` |
| `golden_business_days` | `kestrel_reference/fixtures/business_days.py` |
| `golden_surface_diff` | `kestrel_reference/fixtures/surface_diff.py` |
| `golden_revaluation_diff` | `kestrel_reference/fixtures/revaluation_diff.py` |

The remaining fixtures state their derivation in their own header; they are being brought under
this generator one by one.
