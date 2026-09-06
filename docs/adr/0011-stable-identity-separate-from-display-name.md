---
status: superseded
superseded-by: 0020
---

# The app's identity is a fixed neutral slug, independent of its display name

> **Superseded by [ADR-0020](0020-identity-matches-the-settled-name.md).** The slug
> is now `com.v3sper.takyon`. This ADR's reasoning was insurance against a third
> rename; the name settled, so the premium stopped being worth paying. The migration
> costs listed below are the checklist ADR-0020 worked through — kept because they
> are still what a future rename would owe. Everything after this note describes the
> decision as it stood, not as it stands.

Everything Windows keys off — the MSIX package identity, `%LOCALAPPDATA%\<slug>\`,
the registry `Run` value, the single-instance mutex, the updater feed — uses a
stable neutral slug chosen once and never changed. The product's display name is
a string in UI copy and the installer, and can change freely.

**The slug is `com.v3sper.launcher`**, with data under
`%LOCALAPPDATA%\v3sper\launcher\`. It follows the existing `com.v3sper.*`
convention (cf. `com.v3sper.tesseract`) and deliberately contains no product name.
**The display name is "Takyon"** — a respelling of *tachyon*, the particle that
outruns light, using the same `c`→`k` move that turned dictaphone into Diktafone.

## Why separate them

Not because the name is undecided. Because **naming this product has already
churned twice, so a third rename is plausible enough to insure against.**

- **"Taskmaster"** was dropped: it collides with `claude-task-master`, a widely
  used Claude Code task-management tool — the same audience, an adjacent
  function, and therefore a real confusion risk.
- **"Praxis"** was dropped: crowded past usefulness. ETS's teacher-certification
  exams (large, trademarked in education), Praxis EMR (medical records
  *software*), `google/praxis` (the JAX layer library for Pax — Apache-2.0,
  actively maintained, and squarely *in the developer namespace*), and a European
  DIY retail chain.
- **"Takyon"** was swept before adoption. The developer namespace is clean: PyPI
  free; npm holds a dead 331-byte stub from 2018; crates.io has a dormant
  io_uring async runtime (2.6k downloads, last touched Nov 2023); GitHub's largest
  hits are niche HPC communication libraries at 13 and 6 stars. No launcher, no
  desktop tool, nothing notable. The *brand* namespace is another matter — at
  least four companies (an Italian travel exchange, an Italian cloud-cost startup,
  Takyon AI, and Takyon Networks Ltd, publicly listed in Aug 2025), with
  `takyon.io`, `takyon.dev` and `takyon.app` all live. `takyon.com` did not
  resolve and may be available.

Takyon is clean where the users are and crowded where the marketing would be.
That's an acceptable trade for a tool that may stay personal, and precisely the
kind of trade that can look different in two years.

Note the counter-example: tesseract uses `com.v3sper.tesseract`, product name
included, and that has been fine. This ADR deliberately diverges from that
precedent, for the reason above.

The asymmetry decides it. Decoupling costs one small, permanent legibility hit and
can only be done *before* shipping. Not decoupling costs, if a rename ever
happens, a full migration:

- **MSIX package identity is the app's identity to Windows.** Change it and
  Windows treats it as a different application — users must uninstall and
  reinstall, and package-container data is lost unless explicitly migrated.
- **The data directory** would need a migration on upgrade, or users silently lose
  clipboard history and Frecency.
- **The registry `Run` value** would leave an orphan under the old name, so the
  app either fails to autostart or the old entry lingers pointing at nothing.
- **SmartScreen reputation** accrues per signed binary. A renamed executable
  restarts that accumulation, so users get "unknown publisher" warnings again for
  a while. (The code-signing certificate itself is fine — it carries the publisher
  name, not the product name.)

With this separation, renaming the product later is a UI-copy change and a new
installer title. Without it, it's a migration.

## Consequences

The tradeoff is legibility: someone browsing the registry or `%LOCALAPPDATA%` sees
a slug that may not match the app's name. That's a small, one-time confusion
against a rename that would otherwise be a genuine migration — and it buys the
freedom to keep arguing about the name while building.
