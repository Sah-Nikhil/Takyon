---
status: accepted
---

# Takyon is source-available under CC BY-NC-SA 4.0

Takyon's own code and documentation are licensed under Creative Commons
Attribution-NonCommercial-ShareAlike 4.0 International (`CC-BY-NC-SA-4.0`),
with one additional permission for personal use at work. The name and logo are
outside the licence. This closes the "open source vs proprietary" question that
ROADMAP.md carried since v0.1 and that ADR-0005 cited when it ruled out GPL code.

The research behind this is in `docs/prior-art/licensing.md`, including the
correction described under "Why not PolyForm Noncommercial" below.

## The goal

Anyone may use, study and modify Takyon for their own needs, and contribute
back, with the source public. Nobody, individual or company, may sell it,
charge for copies, repackage it as their own, or use any part of the code for
commercial gain.

## Takyon cannot be called open source

The Open Source Definition, clause 6, forbids restricting "the program from
being used in a business". Every licence that meets the goal fails it. Takyon is
therefore **source-available**, and README, NOTICE and every store listing say
so. Calling it open source would be false.

## The decision

- `LICENSE` is the verbatim CC BY-NC-SA 4.0 legal code. Never edited: an edited
  text loses the SPDX id and the drafting it was chosen for.
- `NOTICE` carries the copyright line, the scope, and an **additional
  permission**: any individual may run Takyon as their own productivity tool,
  including at work. Without it, "not primarily intended for or directed
  towards commercial advantage" plausibly excludes someone using a launcher at
  their job, which is most of a launcher's users. The permission allows running
  only, never distributing, bundling or reusing code.
- `TRADEMARK.md` covers the name and logo, which the licence leaves out
  ("Patent and trademark rights are not licensed under this Public License").
  ™ until registered, never ® before: false representation as registered is an
  offence under India's Trade Marks Act 1999, s.107.
- `CONTRIBUTING.md` holds contribution terms, accepted through a checkbox in
  the pull request template rather than a signed CLA. Contributors grant a
  broad copyright licence, including the right to relicense under any terms,
  and a patent licence. The terms state no reason; they mean what they say.
- Third-party notices ship inside every installer as `THIRD_PARTY_NOTICES.txt`,
  generated at build time. MIT and Apache-2.0 require this regardless of
  Takyon's own licence, and every release so far shipped without it.

## Why not PolyForm Noncommercial 1.0.0

It was the first recommendation: written for software, lawyer-drafted, with a
patent grant. Reading the text closely killed it. Its copyright and changes
licences are limited to "any permitted purpose", but its Distribution License
is not:

> The licensor grants you an additional copyright license to distribute copies
> of the software.

Read plainly, anyone may distribute copies and charge for them. Buyers get only
noncommercial rights, but the seller keeps the money, which is exactly what the
goal forbids. Its noncommercial-organisation clause widens the same gap for
charities, schools and governments. Fixing either means editing the text,
which makes it a custom licence.

CC BY-NC-SA limits every grant, sharing included, to NonCommercial purposes,
defined as "not primarily intended for or directed towards commercial
advantage or monetary compensation". Charging for a copy is monetary
compensation, for everyone, with no institutional carve-out.

## Considered options

- **PolyForm Noncommercial 1.0.0**: rejected, above.
- **PolyForm Strict 1.0.0**: forbids modification, so nobody could contribute.
- **PolyForm Shield, Elastic License 2.0, Commons Clause, FSL**: ban only
  competing products, hosted services or selling the software as such. All
  other commercial use, including lifting code, stays allowed.
- **Business Source License 1.1, FSL**: convert to an open source licence after
  two to four years. The goal is permanent.
- **AGPL-3.0, MPL-2.0**: open source, so they allow resale by definition.
- **A custom licence**: no SPDX id, no tooling recognition, and drafting risk
  for a solo author with no lawyer.

## What CC BY-NC-SA costs

- **Creative Commons advises against it for software.** It says nothing about
  source code versus binaries and is incompatible with software licences.
  Neither bites here: dependencies keep their own MIT and Apache-2.0 licences,
  and no GPL code may enter (below).
- **No patent grant from the licensor.** V3SPER holds no patents, so users
  lose nothing today. Contributors' patents are covered by the contribution
  terms.
- **GitHub will not name the licence** in the repository sidebar. Its detector
  knows no noncommercial licence. Cosmetic; the SPDX id is in README and the
  manifests.

## What no licence closes

- Nobody who modifies Takyon has to publish their changes. ShareAlike binds
  only what they share.
- Clean-room reimplementations and ideas are not protected by copyright.
- "Noncommercial" has fuzzy edges in every licence that uses it.
- Enforcement is a takedown notice or a lawsuit, both on the author.

## Consequences

- The GPL dependency ban stays, for a new reason: GPL forbids adding
  restrictions, so a GPL component cannot ship inside a noncommercial binary.
  `CC-BY-SA` and `CC-BY-NC` dependencies stay banned too, because their own
  ShareAlike or NonCommercial terms would bind the combined work.
  `scripts/check-licences.ts` enforces all of it.
- Manifests declare `CC-BY-NC-SA-4.0`. `check-licences` skips Takyon's own
  packages, so its own copyleft pattern never flags them.
- Selling a commercial licence, or moving to an open source licence later,
  stays possible because every commit so far is the author's and contributions
  arrive under the contribution terms.
- Releases before this decision shipped with no licence, which means all
  rights reserved. The licence applies to every version from here on.
- A lawyer should review the additional permission and the contribution terms
  before outside contributions are merged.
