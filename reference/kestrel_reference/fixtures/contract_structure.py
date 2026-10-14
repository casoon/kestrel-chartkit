"""contract_structure: complete per-bar contracts of the structure family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code.
"""

from ..fixture import provenance
from .contract_pilot import case_block

NAME = "contract_structure"


def build():
    header = provenance("contract_structure") + ["", "Format: see contract_pilot."]
    blocks = []
    return header, blocks
