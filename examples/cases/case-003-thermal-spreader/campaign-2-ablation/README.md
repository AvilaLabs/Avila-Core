# CASE-003 campaign 2: EXP-002 Core-feedback ablation, block 1

Raw records of the scored block described and interpreted in
[`experiments/EXP-002-core-feedback-ablation.md`](../../../../experiments/EXP-002-core-feedback-ablation.md).

- `block-1/` — the fifteen scored trials (five per arm, seeded order),
  each with `config.json` (identities, budgets, timing, usage),
  `transcript.jsonl` (the designer session, stream JSON), `designer/`
  (its proposals and tool state), `evaluation-log.jsonl`, the post-hoc
  Core pass (`post-hoc-core/`) and, for arm A, every live Core workspace
  with its receipts; `block-summary.json`.
- `block-1-limit-failed/` — the first attempt at the same block, every
  trial refused by the operator account's session limit (amendment A1);
  kept unscored.
- `scores.json`, `scores.md` — the deterministic scorer's output.
- `run_block.sh` — the exact driver, including the frozen command.

The Core binary the block ran under is not archived (it is a build of
commit `ee25b71`; its SHA-256 is in every `config.json`).
