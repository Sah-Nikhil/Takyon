---
status: watching
pairs-with: docs/plans/v0.4-calculator.md task 1
---

# TBC-0011 — A hand-rolled expression parser instead of a crate

## The bet

The v0.4 plan said to pick a Rust expression crate rather than hand-roll a
parser. We hand-rolled it: `sources/calc/` carries its own tokenizer, a
precedence-climbing parser and an evaluator, in the low hundreds of lines, with
no new dependency.

The assumption is that **the arithmetic is the smallest and least interesting
part of this feature.** What a launcher's calculator actually has to get right is
policy — when to answer at all, and how to present the answer — and no crate
ships any of it:

| Behaviour | Whose problem |
|---|---|
| `45+23` → `68` | the crate's, and it is solved everywhere |
| `10+30%` → `13`, not `10 % 30` | ours; every language reads `%` as remainder |
| `2024` → `2,024` but `202` → nothing | ours |
| `1password` → no Calc Entry at all | ours |
| `40 kg to lb` | ours; no arithmetic crate knows `kg` or `to` |
| `=45+23` forcing a calculation | ours |

A crate would have covered one row of that table. The other five need a
tokenizer that understands units and percent, which is the same tokenizer the
arithmetic needs — so buying the arithmetic means running two parsers over one
string, each with its own idea of what a token is.

The second half of the bet is licensing. Distribution is undecided and ADR-0005
keeps us clear of GPL, so the crates worth having are the ones that force the
question early: `rink-core`, the one library that would have solved units and
currency together, is MPL-2.0.

## How we'd know we were wrong

- A correctness bug in the arithmetic itself — precedence, associativity, unary
  minus, float formatting — reaches a release. **One is a bug; a second is this
  note triggering**, because it means the "textbook algorithm, easy to test"
  claim was wrong.
- The grammar needs something the parser was not shaped for: variables, an answer
  history (`ans * 2`), complex numbers, arbitrary precision, date arithmetic
  (`today + 3 weeks`), or bases and bitwise work beyond a couple of operators.
- Maintaining it starts costing more than a day per phase.
- Currency arrives at v0.8 and wants a units engine deep enough that ours is the
  thing standing in the way.

## Alternatives

| Option | Improvement if we switch | Added complexity | Switching cost |
|---|---|---|---|
| **Keep it** | none; ~300 lines of pure logic under unit test | none | 0 d |
| `evalexpr` (MIT) | arithmetic maintained by someone else | a dependency, plus the same policy layer, plus a percent rewrite and a second tokenizer for units | 1 d |
| `fasteval` (MIT/Apache-2) | as above, and faster than we need to be | as above | 1 d |
| `rink-core` (MPL-2.0) | units and currency solved in one, with a real dimensional-analysis engine | a large unit database to load against the 30 ms budget, a famously greedy parser to restrain, and an MPL-2.0 answer owed before we ship | 3–5 d |

## Verdict if triggered

Depends which trigger fired.

**Arithmetic bugs**: swap the evaluator, keep everything else. `calc::eval` is the
seam, and the policy, formatting and unit layers sit outside it — so this is a
one-file replacement, which is the main reason the module is drawn this way.

**Grammar outgrown**: same swap, and `evalexpr` is the pick, on licence and on
being the least surprising.

**Currency at v0.8 wanting a real units engine**: that is the one case worth
reconsidering `rink-core` wholesale, and it needs the ADR-0005 licence question
answered first rather than alongside.
