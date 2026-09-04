# Third-party notices

This inventory covers third-party material included in, or used to generate
material included in, this source repository. It does not replace the terms of
the identified licenses and is not a legal opinion.

## FISPACT-II 709-group boundaries

`examples/capabilities/shield-coupled/fispact-709-groups.json` is an
ascending-order JSON transformation of `GROUP_709` from the FISPACT-II
`pypact` project. The source file says the group structures were taken from the
FISPACT-II 4.0 manual.

- Upstream: <https://github.com/fispact/pypact>
- Copyright: 2018 FISPACT-II
- License: Apache License 2.0
- Local modifications: reversed the descending source order and wrapped the
  values in an Avila shielding group-structure document
- License text: [`LICENSES/Apache-2.0.txt`](LICENSES/Apache-2.0.txt)

## OpenMC-generated multigroup artifact (not distributed)

The repository's `mgxs_build.py` script can generate
`mgxs-vitamin-j-175.h5` with OpenMC. The reference sidecar records a build
made with OpenMC 0.15.3, including its parameters and the digest of the
ENDF/B-VII.1 cross-section index. The generated HDF5 file itself is
intentionally not distributed or tracked.

OpenMC is distributed under the MIT license:

> Copyright (c) 2011-2025 Massachusetts Institute of Technology, UChicago
> Argonne LLC, and OpenMC contributors
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to
> deal in the Software without restriction, including without limitation the
> rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
> sell copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
> SOFTWARE.

A locally generated HDF5 artifact also contains neutron dose-response
coefficients collapsed from the ICRP Publication 116 table distributed with
OpenMC. ICRP states that reproducing material from its reports requires
permission. Avila Labs has not documented that permission or another
redistribution basis, so this repository distributes the builder and reference
sidecar but not the generated artifact. Anyone redistributing a generated
artifact must separately establish the necessary rights.

Sources:

- <https://github.com/openmc-dev/openmc/blob/v0.15.3/LICENSE>
- <https://docs.openmc.org/en/v0.15.3/pythonapi/generated/openmc.data.dose_coefficients.html>
- <https://openmc.org/data/>
- <https://www.icrp.org/page.asp?id=11>

The AGPL license for Avila Core does not relicense OpenMC, ENDF/B, ICRP, or
other upstream material.

## NAFEMS thermal benchmark reference

`examples/capabilities/thermal/validation/nafems_t4.py` is an independent
implementation that cites and reproduces the numerical result of NAFEMS Test
4M. The repository does not include the NAFEMS publication, figures, or source
text. NAFEMS identifies the thermal benchmark collections at
<https://www.nafems.org/publications/glossaryofbenchmarks/thermalanalysis/>.

## SIMSOPT NCSX reference inputs (not distributed)

CASE-005's included result was generated from the NCSX modular-coil
coefficients and plasma-boundary coefficients in the public SIMSOPT repository
at commit `c648630cfc5625863b291709c17015bcdcba13af`. The source files are
content-identified by the case package but are not copied into this repository.

- Upstream: <https://github.com/hiddenSymmetries/simsopt>
- Source files: `src/simsopt/configs/NCSX.dat` and
  `tests/test_files/input.NCSX_c09r00_halfTeslaTF`
- Copyright: 2023 SIMSOPT contributors
- License: MIT
- Local use: independent Biot–Savart sensitivity and passive-stiffness ceiling
  calculation; no upstream code is imported or executed
- License text: <https://github.com/hiddenSymmetries/simsopt/blob/c648630cfc5625863b291709c17015bcdcba13af/LICENSE>

The generated CASE-005 result is not endorsed or validated by the SIMSOPT
contributors or NCSX institutions.

## Rust dependencies and bundled fonts

Rust dependencies are resolved by `Cargo.lock` and are not vendored in this
repository. Their licenses remain their own. A distributed binary must carry
the notices required by its resolved dependency set, including the OFL-1.1 and
Ubuntu Font License material reported by `epaint_default_fonts`; the root AGPL
file is not a substitute for those notices.
