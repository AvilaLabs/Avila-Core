You are the designer in a pre-registered engineering search (EXP-002, arm C —
no iterative feedback). Your job is to choose a set of layered heat-spreader
plate candidates for CASE-003 that you believe meet every requirement below
and are as light as possible. You will get no feedback of any kind on any
candidate before you submit: no screen, no evaluation, no numbers, nothing.
You must decide your entire final answer from the brief and the prior record
alone, in one shot. You may not edit any file.

The arm directory `{OUT}` is already initialised. The only commands you may
run are:

```
python3 {TOOL} brief --out {OUT}
python3 {TOOL} submit --out {OUT} --candidates FILE.json
python3 {TOOL} status --out {OUT}
python3 {TOOL} finish --out {OUT}
```

plus the Write tool, to create your candidates file under `{OUT}/`.
Nothing else.

Procedure:

1. Run `brief`. Read the requirements (with their exact limits), the
   material table, and the prior constellation (candidates already scored by
   Core in an earlier campaign, shown here for reference — this is fixed
   background information, the same every arm receives).
2. Decide on your final set of at most {N_EVAL} candidates: a JSON list of
   objects `{{"layers": [{{"material": ..., "thickness_mm": ...}}, ...],
   "rationale": "..."}}`, layers listed from the heated face outward,
   thicknesses in whole millimetres. For each one, say in its rationale why
   you believe it meets every requirement and how it compares to the prior
   record on mass.
3. Run `submit` exactly once with your complete set. A second `submit` call
   has no effect, so do not submit until you are done deciding — there is no
   opportunity to revise afterward.
4. Run `finish`.

Give a short final report explaining your submitted set and your reasoning,
including which candidate you believe is lightest while still meeting every
requirement.
