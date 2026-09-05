You are the designer in a pre-registered engineering search (EXP-005, arm B
— raw solver feedback). Your job is to find a layered slab shield for
CASE-002 that meets every requirement below and is as light as possible.
There is no automated judge in this arm: the tool you run reports only raw
physical numbers from the screening, transport, and activation scripts. It
does not tell you whether a design passes, does not compare a number to a
limit, and does not compute a margin. You must do that arithmetic yourself,
against the requirement limits printed by `brief`. You may not edit any
file.

The arm directory `{OUT}` is already initialised (budgets and script paths
are fixed). The only commands you may run are:

```
python3 {TOOL} brief --out {OUT}
python3 {TOOL} propose --out {OUT} --proposals FILE.json
python3 {TOOL} evaluate --out {OUT} --rationale "..." ID [ID ...]
python3 {TOOL} status --out {OUT}
python3 {TOOL} finish --out {OUT}
```

plus the Write tool, to create proposal files under `{OUT}/proposals/`
(the directory will be created for you the first time you write into it).
Nothing else.

Procedure:

1. Run `brief`. Read the requirements (with their exact limits), the
   material table, and the prior constellation (candidates already scored by
   Core in the control sweep, shown here for reference — this is fixed
   background information, the same every arm receives).
2. Write a proposal file: a JSON list of up to 10 objects
   `{{"layers": [{{"material": ..., "thickness_cm": ...}}, ...], "rationale": "..."}}`,
   layers listed from the source outward, thicknesses in whole centimetres.
   Run `propose` on it — this runs the one-dimensional screen script only
   and reports its raw dose-rate estimate, areal mass, and thickness.
3. Choose which screened candidates to fully evaluate (at most 3 per round)
   and run `evaluate` with a rationale. This additionally runs the coupled
   neutron-photon transport and the layer activation scripts and reports
   their raw numbers: neutron dose rate, photon dose rate (each with the
   statistical interval the transport script itself reports), and the
   highest layer specific activity. It costs about four to six minutes per
   candidate. Compare every number yourself against the requirement limits
   from `brief` — the tool will not do this for you.
4. Repeat from step 2 using every result so far. Stop when your evaluation
   budget ({EVAL_BUDGET}) is spent, or when you judge further evaluations
   will not produce a design you believe meets every limit while being
   lighter; say why.
5. Run `finish`.

You may use up to four layers in a design; there is no other stated limit
on layer count.

Rules that are not negotiable: propose in whole centimetres; never repeat a
design already evaluated; state your own judgment of whether a design meets
every requirement, and say so explicitly, since the tool will not. When you
stop, give a short final report: the lightest design you believe meets
every requirement and at which evaluation it appeared, and what you would
try next with more budget.
