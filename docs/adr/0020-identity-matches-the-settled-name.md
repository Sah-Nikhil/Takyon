---
status: accepted
supersedes: 0011
---

# The identity slug is `com.v3sper.takyon`, matching the settled display name

Everything Windows keys off — the package identity, `%LOCALAPPDATA%\v3sper\takyon\`,
the registry `Run` value, the single-instance mutex, the UIAccess pipe, the updater
feed — uses the slug `com.v3sper.takyon`. The display name stays "Takyon", and the
two are still separate literals in separate places, but they now read alike.

This supersedes ADR-0011, which chose the deliberately neutral `com.v3sper.launcher`.

## Why the earlier decision no longer holds

ADR-0011 was not about aesthetics. It was insurance against a third rename, taken
out because naming this product had already churned twice: "Taskmaster" was dropped
over a collision with `claude-task-master`, and "Praxis" was dropped as hopelessly
crowded. A neutral slug meant a third rename would cost a string change in UI copy
rather than a migration.

The premise has expired. "Takyon" has survived a namespace sweep, shipped through
nine releases, and is now the name in the README, the changelog, the installer and
a public hackathon pitch. The thing being insured against is no longer plausible
enough to keep paying for, and the premium — a registry key and a data directory
that do not obviously belong to the app anyone is looking at — is paid by every
person who ever opens Regedit or `%LOCALAPPDATA%` wondering where Takyon keeps its
clipboard history.

It also aligns this product with its sibling. Tesseract uses `com.v3sper.tesseract`,
product name included, and ADR-0011 called that out as the counter-example it was
knowingly diverging from. There is now no reason for the divergence.

## Why now rather than later

Because the cost only grows, and it is currently near zero.

ADR-0011 listed what a rename would cost, and today every item on that list is
either theoretical or confined to one machine:

- **Package identity.** No MSIX target exists; the bundle is NSIS only. Windows
  keys the single-instance mutex off `tauri.conf.json`'s identifier, which is a
  string change with no installed-base consequence.
- **The data directory.** One machine has data under the old path. `identity::
  migrate_legacy_data_dir` renames it in place on startup, which is atomic because
  both paths share a parent.
- **The registry `Run` value.** The NSIS `POSTINSTALL` hook deletes the old pair on
  upgrade, and `POSTUNINSTALL` deletes both pairs, so no orphan survives either
  path.
- **SmartScreen reputation.** None has accrued: nothing is code-signed yet, which
  is exactly why this is the cheapest moment to change the name Windows sees.

Every one of those becomes real the day the product is signed and distributed. The
decision is therefore: rename before signing, or never.

## What is deliberately left spelled `launcher`

Three things keep the old string on purpose, and removing any of them destroys
user data silently rather than loudly.

- **`clips::key::LEGACY_ENTROPY` and `search::key::LEGACY_ENTROPY`.** DPAPI entropy
  is an input to decryption, not a label. A clipboard key wrapped under the old
  entropy will not unwrap under the new one, and the failure surfaces as a history
  that looks empty rather than as an error. Both call sites try the current
  entropy, fall back to the legacy one, and rewrap in place, so the fallback pays
  one failed DPAPI call exactly once per machine.
- **`prefs.ts`'s `LEGACY_MOTION` and `LEGACY_CALC`.** These name `localStorage`
  entries already written under the old slug. Renaming the constants would not
  rename the entries; it would just stop finding them.
- **The NSIS hooks' `com.v3sper.launcher` deletes.** They exist precisely to clean
  up what the old name left behind.

The two `LEGACY_ENTROPY` constants can be deleted once no machine still holds a
pre-rename key. Nothing tracks that, so in practice they stay.

## Consequences

The legibility argument in ADR-0011's "Consequences" section inverts: the registry
and `%LOCALAPPDATA%` now name the app you are looking at.

The cost is that the guard rail is weaker. ADR-0011 could assert mechanically that
the slug did not contain the display name, and that assertion caught a derived slug
before it could reach the registry. That test is gone, because the strings now
match by design. What replaces it is weaker but not nothing: `IDENTITY` is asserted
to be the exact literal `com.v3sper.takyon`, so building it with
`format!("com.v3sper.{}", DISPLAY_NAME.to_lowercase())` still fails the suite even
though it would produce the right answer today. A future rename must therefore
still be a deliberate edit to this constant, and must still write the migration
that goes with it.
