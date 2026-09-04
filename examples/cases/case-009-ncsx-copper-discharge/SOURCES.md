# CASE-009 source and prior-art notes

These notes distinguish source facts from model inferences. Page numbers are
PDF page numbers unless a slide is named.

## NCSX and copper data

- The [NCSX Engineering Design Document](https://ncsx.pppl.gov/Meetings/CDR/CDRFinal/SDD_WBS1_C.pdf),
  p. 7, states that all NCSX coil systems were cryoresistive and operated at
  liquid-nitrogen temperature.
- [TechnicalData2](https://ncsx.pppl.gov/Meetings/CDR/CDRFinal/TechnicalData2.pdf),
  p. 23, identifies modular-coil reference `m45r00` and supplies the six-coil
  type grouping, 36 turns/coil, centerline lengths, copper area, helical-length
  correction, 85 K resistivity, and rounded initial resistances. The model's
  1.05843 length factor is derived from the published 5.843% correction.
- The same source, pp. 26–30, supplies the pulse current/temperature history,
  deposited energy, final temperature, line terms, exact winding resistances,
  self-inductances, 24 kA rating, and I²t values used to set or validate the
  model. The resistances and inductances are explicitly for each six-coil
  series circuit; the line terms are design assumptions. The 5.3e8 A²s circuit
  rating is used only as a provisional screen, not as a demonstrated
  fault-survival limit.
- The [NIST OFHC-copper page](https://trc.nist.gov/cryogenics/materials/OFHC%20Copper/OFHC_Copper_rev1.htm)
  provides the heat-capacity fit and states a 5% fit error above 15 K. CASE-009
  propagates its 0.95/1/1.05 scale endpoints. [NIST Monograph 177](https://doi.org/10.6028/NIST.MONO.177)
  provides the intrinsic resistivity fit; CASE-009 anchors its shape to the
  NCSX 85 K value and uses a chosen ±15% sensitivity rather than claiming that
  interval as NIST uncertainty.

The conceptual source says copper but does not establish OFHC grade. NIST OFHC
properties and 8960 kg/m3 density are therefore explicit model assumptions.
The fault begins at the table's 0.213 s state, immediately before its labeled
0.218 s high-beta row. No fault-onset-time sweep is performed, so the absolute
temperature and I²t results are not a worst-time-in-pulse envelope.

## Revision and voltage cautions

The later [2004 modular-coil final design review](https://ncsx.pppl.gov/NCSX_Engineering/ModCoil_TF-Coil_VVSA_Fab/Jobs1451_1459_1408/011-May%20FDR%20NCSX/FDR%20Presentations/mcwf_fdr_wms.pdf)
uses 20/20/18 turns and other changed production parameters. Its slide 20 lists
a 2 kV operating voltage but does not state whether that is terminal-to-terminal
or terminal-to-ground. CASE-009 uses only that value as a provisional screen;
it does not mix the later conductor or winding values into `m45r00`.

The chosen 0.05 ohm private dump, converter-collapse fault, delay set, and
global source bypass are exploratory model choices, not sourced NCSX
protection settings. The [WBS-4 closeout](https://ncsx.pppl.gov/NCSX_Engineering/CloseOut_Documentation/Ramakrishnan/WBS4_Jobs/CloseoutNotes%20WBS4%20092508.pdf)
records that protection/control design remained preliminary, its PDR was not
held, and the project was terminated. No operational NCSX fault-discharge data
validate this model.

The approved [modular-coil system requirements document](https://ncsx.pppl.gov/SystemsEngineering/Requirements/Specs/WBS1/WBS14/WBS14_SRD/Rev1/NCSX-BSPEC-14-01-Signed.pdf)
gives voltage-test multipliers but no public numeric operating voltage to
ground. The later fault document's single-coil-short and zero-current-string
cases are explicitly labeled a
[fault-mode strawman](https://ncsx.pppl.gov/NCSX_Engineering/Analysis_Reports/Operating%20Scenarios%20for%20Analyses%20%20Fault%20Mode%20Analyses.pdf),
not an approved requirement. CASE-009 does not claim to simulate either case.

## Prior-art boundary

Broad protection claims are crowded. Relevant examples include:

- [US8482369B2 / US20130106545A1](https://patents.google.com/patent/US20130106545A1/en):
  resistor ladders across corresponding symmetric coils to balance currents and
  reduce quench force;
- [US6717781B2](https://patents.google.com/patent/US6717781B2/en):
  symmetric coils with resistor/heater paths that retain similar currents;
- [US5644233A](https://patents.google.com/patent/US5644233A/en):
  geometry-aware coil grouping and protection shunts;
- [US6563316B2](https://patents.google.com/patent/US6563316B2/en):
  coil-section selection using self/mutual coupling and stray-field objectives;
- [US8780510B2](https://patents.google.com/patent/US8780510B2/en):
  passive voltage-triggered protection paths; and
- [CLIQ](https://cds.cern.ch/record/2145871/files/10.1016_j.phpro.2015.06.037.pdf?version=1):
  active deliberate differential-current excitation governed by self/mutual
  inductance and winding partition;
- [Yunus, Iwasa, and Williams (1995)](https://doi.org/10.1016/0011-2275(95)92876-T):
  resistor-shunted, inductively coupled multicoil quench behavior; and
- [Guo et al. (2010)](https://www.osti.gov/servlets/purl/981463-dQWe70/):
  passive quench-back and resistor-shunted coil subdivision.

The remaining research island is narrow and is not established novelty:
resistive-network/Laplacian synthesis across independently powered,
non-axisymmetric stellarator circuits, jointly constrained by full-3D field,
inter-coil filament-force, voltage, thermal, and fault metrics. CASE-009 supplies no
unexpected positive result for that island; its tested delayed symmetric mesh
is negative evidence. A patent conclusion would require a materially different
working topology, a claim chart, and professional review.
