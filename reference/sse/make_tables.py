#!/usr/bin/env python3
"""Turns the coefficient tables of SSE's zdata.h into Rust constants.

Usage: make_tables.py path/to/zdata.h > tables-body.rs

Only the main-sequence tables are converted (xz, xt, xl, xr). Every value is
checked to be numerically identical to the Fortran literal it came from.
"""
import re
import sys

SIZES = {"xz": 76, "xt": 31, "xl": 72, "xr": 119}

source = open(sys.argv[1]).read()
for name, count in SIZES.items():
    body = re.search(r"data\s+%s\s*/(.*?)/" % name, source, re.S).group(1)
    body = re.sub(r"\n\s{5}&", " ", body)
    tokens = [t.strip() for t in body.replace("\n", " ").split(",") if t.strip()]
    assert len(tokens) == count, (name, len(tokens))
    values = []
    for token in tokens:
        mantissa, exponent = re.fullmatch(r"(-?\d*\.?\d*)d([+-]\d+)", token).groups()
        if mantissa.endswith("."):
            mantissa += "0"
        values.append(f"{mantissa}e{int(exponent)}")
        assert float(values[-1]) == float(token.replace("d", "e"))
    print(f"pub(super) const {name.upper()}: [f64; {count}] = [")
    for i in range(0, count, 4):
        print("    " + ", ".join(values[i : i + 4]) + ",")
    print("];\n")
