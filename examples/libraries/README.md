# Libraries

A library is the owned, versioned content a domain contributes to Core:
registry content (kinds, roles, purposes, capability types), capability
adapters, contract templates, and, first among them here, a **requirement
set**: what any contract in the domain must address, so that a search cannot
optimize an incomplete question.

`shielding/requirement-set.json` is the first requirement set. CASE-001 keeps a
byte-identical copy as its `requirement_set` document and declares in its
package which contract requirements cover each entry and why the rest are
omitted, with the owner who accepted each omission. `avila-core run` assesses
that coverage after compiling and before executing anything; an entry the set
says must be stated, left uncovered without a statement, stops the run.

A library also owns its capabilities' qualification records (ADR-0008):
CASE-001's transport record lives with the case for now, binds no
validation evidence, and says so.

Nothing here is qualified. The shielding set is written by the case author for
a research specimen; a real library's set is owned and reviewed by the domain's
professionals, and its identity is what a reviewer cites.
