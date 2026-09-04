# CASE-007 protocol: fault-balanced winding partitions

**Status:** exploratory iteration run on 2026-09-04.

**Registration note:** the 1.25 magnetic and 1.10 force-reduction thresholds
were fixed during interactive work on the reference mesh, not committed before
all reference results were observed. The candidate was frozen before the fine
mesh was evaluated. This is a reproducible exploration with a numerical
holdout, not a preregistered confirmation.

## Question

Can the 18 physical coils generated from the three public NCSX base-coil types
be assigned to three equal winding circuits so that complete loss of any one
circuit produces less magnetic-shape and distributed-force departure than a
type-segregated three-circuit control?

## Fixed architecture and fault

- Three circuits contain six coils each.
- Every candidate circuit contains two copies of each base-coil type.
- Type-dependent turn counts are assumed to reproduce the published nominal
  ampere-turn ratios under a common circuit current.
- The endpoint removes one entire circuit while the other two remain at
  nominal current.
- Twenty-one proportional current-loss samples, including both endpoints,
  audit the same fault path.
- The conceptual control assigns all six symmetry copies of each base-coil
  type to one circuit.

The experiment does not claim that this was the historical NCSX circuit
architecture.

## Search and feedback

The reference grid uses 128 segments per coil and a 24 by 16 plasma-boundary
surface. All 121,500 balanced architectures, unique up to circuit labels, are
evaluated. Rounded objective values and then physical-coil indices provide a
deterministic tie break.

The first objective minimizes only worst magnetic-shape departure. Its force
reduction is compared with the already fixed 1.10 gate. On failure, the repair
does not change the circuit counts, candidate space, comparator, fault, or
gates; it changes selection to minimize the worse of magnetic and force
departures after each is normalized by the control.

The selected repair is then evaluated without reselection at 256 segments per
coil and a 48 by 32 surface.

## Metrics

For current fractions `x`, the common-current reference is the total absolute
ampere-turn-weighted mean `alpha`.

- Magnetic-shape departure is the weighted RMS surface-normal field difference
  between the fault state and `alpha` times the nominal field, normalized by
  nominal RMS field magnitude.
- Force departure is the coil-length-weighted RMS difference between the
  fault-state filament force density and `alpha^2` times the nominal force
  density, normalized by nominal RMS force density.
- Peak-force ratio compares the greatest sampled local filament force density
  with its nominal maximum.

The reduction factors divide the control's worst circuit by the candidate's
worst circuit. These are proxies for screening circuit assignments, not plasma
confinement or structural stress.

## Gates

- fine magnetic reduction at least 1.25;
- fine force reduction at least 1.10;
- sampled peak-force ratio no greater than 1;
- no path departure more than 1.01 times its complete-loss endpoint;
- reduction-factor change between reference and fine meshes no greater than
  0.02; and
- relative spread among the three candidate fault cases no greater than
  1e-9.

The selected groups must also partition all 18 indices once and contain two
coils of each type per circuit.

## Decision rule

Advance only to a coupled inductance, resistance, voltage, thermal-quench, and
structural model if every requirement passes. A pass does not justify hardware
or an intellectual-property claim. A coupled model must retain the advantage
under realizable integer turns, winding packs, dump networks, detection delay,
hot-spot and voltage limits, and stress—not merely reproduce this
magnetostatic proxy.
