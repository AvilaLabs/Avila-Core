# Synthetic external-checker transport fixtures

`numeric-uncertainty.adapter.json` declares one interval with an optional
nominal pointer and one numeric unquantified estimate. Its companion output
uses canonical decimal/rational strings. No executable, physics model or
scientific qualification is supplied by these files.

The runner's `external_checker::numeric_tests` consumes this pair. Portable
tests exercise descriptor and output validation; Unix integration tests bind
a synthetic copying executable and test receipts, qualification, requirement
boundaries and reuse. See [ADR-0017](../../docs/adr/0017-external-checker-numeric-uncertainty.md)
for the transport semantics and migration boundary.
