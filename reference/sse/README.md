# SSE reference for the main-sequence fits

`sg-stellar::evolution` implements the analytic stellar-evolution fits of
Tout et al. (1996, MNRAS 281, 257) and Hurley, Pols & Tout (2000, MNRAS 315,
543) for the main sequence. The authors distribute a Fortran implementation,
SSE. It is used here in two ways, and none of its source is in this
repository:

1. **Coefficients.** `crates/sg-stellar/src/evolution/tables.rs` is generated
   from SSE's `zdata.h`, which holds the coefficient tables of the two
   papers' appendices, by `make_tables.py`.
2. **Test values.** `crates/sg-stellar/tests/data/sse_ms_reference.txt` is
   the output of `ms_reference.f` (ours) linked against SSE: luminosity,
   radius and main-sequence lifetime for 12 masses, 3 metallicities and 6
   ages. `tests/sse_reference.rs` requires agreement to 1 part in 10¹⁰.

SSE carries no formal licence; its authors ask that the papers be cited.
What this project takes from it is published numbers (the coefficient
tables) and numerical output used as test data. The Rust code is written
from the equations in the papers.

## Regenerating

Needs `gfortran` and `python3`. SSE is fetched from the copy in the AMUSE
repository (`amusecode/amuse`, `src/amuse_sse/src/`):

```sh
mkdir -p /tmp/sse && cd /tmp/sse
base=https://raw.githubusercontent.com/amusecode/amuse/main/src/amuse_sse/src
for f in zcnsts.f zfuncs.f zdata.h star.f hrdiag.f mrenv.f const_bse.h; do
  curl -sLO "$base/$f"
done
cp <repo>/reference/sse/ms_reference.f .

# test values
gfortran -std=legacy -O0 -ffp-contract=off -w -o ref \
  ms_reference.f zcnsts.f zfuncs.f star.f hrdiag.f mrenv.f
./ref   # prepend the three comment lines of the existing file

# coefficient tables
python3 <repo>/reference/sse/make_tables.py zdata.h   # body of tables.rs
```
