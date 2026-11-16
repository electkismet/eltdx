"""Author Rust source data from the standard-library Python gbk codec only.

Run once with an isolated interpreter: python -I -S generate_gbk_pairs.py
This generator never imports the project, invokes a test, or compares implementations.
Zero denotes an undefined two-byte mapping, never a decoded NUL character.
"""

import sys
from pathlib import Path


def main():
    output = Path(__file__).with_name("gbk_pairs.rs")
    lines = [
        "// Generated source DATA from the Python standard-library gbk codec.",
        "// Generator: crates/eltdx/src/encoding/generate_gbk_pairs.py",
        "// Recipe: python -I -S crates/eltdx/src/encoding/generate_gbk_pairs.py",
        "// Executable: " + sys.executable,
        "// Python: " + sys.version.replace("\n", " "),
        "// Codec: gbk; strict two-byte decoding. Undefined entries are zero.",
        "// Index: (lead - 0x81) * 190 + trail - 0x40 - (trail > 0x7f).",
        "pub(super) const GBK_PAIRS: &[u32] = &[",
    ]
    for lead in range(0x81, 0xFF):
        values = []
        for trail in range(0x40, 0xFF):
            if trail == 0x7F:
                continue
            try:
                scalar = ord(bytes((lead, trail)).decode("gbk", errors="strict"))
            except UnicodeDecodeError:
                scalar = 0
            values.append(scalar)
        lines.append("    // Lead 0x%02x" % lead)
        for start in range(0, len(values), 10):
            lines.append("    " + " ".join("0x%04x," % value for value in values[start:start + 10]))
    lines.append("];\n")
    output.write_text("\n".join(lines), encoding="utf-8", newline="\n")
    print("Generated GBK source data: " + str(output))


if __name__ == "__main__":
    main()
