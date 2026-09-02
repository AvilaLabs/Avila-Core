# ADR-0009: Staged agent review

**Status:** superseded 2026-09-02 by
[ADR-0010](0010-technical-verdicts-and-optional-presentation-gates.md)

## Historical context

This decision introduced a useful idea: identified software could inspect an
exact post-campaign dossier, follow explicit practical instructions, and return
a candidate to an autonomous design loop. It correctly kept the agent's routing
record separate from technical evidence.

It also preserved an incorrect premise from the earlier model: that an
accountable human review was universally required to release technical `PASS`.
That premise conflated requirement evaluation with presentation and made Core
depend on professional review for every successful campaign.

Do not implement the old gating rule. The current rule is ADR-0010:

- Core's technical verdict is complete without human or professional review;
- a connected-agent practicality check is optional;
- when configured, it controls whether the surrounding agent returns a
  candidate or presents it to the user;
- its dispositions are `present_to_user`, `request_changes`, and `abstain`; and
- user acknowledgement or organization-specific approval remains outside the
  technical verdict.

The executable migration is preserved in the ADR-0010 consequences and tests.
