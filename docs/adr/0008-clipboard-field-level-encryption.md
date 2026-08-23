---
status: accepted
pairs-with: ADR-0006
---

# Clipboard content is encrypted per-field, not with an encrypted database

Clipboard content is encrypted in the content column only, using a key protected
by Windows DPAPI. The database file itself is ordinary SQLite: timestamps, source
application names and content lengths remain in plaintext. We did not use
SQLCipher.

The reason is dependency posture, not cryptography. SQLCipher would encrypt
metadata and the WAL too, which is genuinely better, but it is a native
dependency to build, ship and keep current, and it needs a licensing review
against a question that is still open — whether Takyon ships open source or
proprietary. Field-level encryption needs no new native dependency and can be
upgraded to SQLCipher later without changing the threat model's conclusion.

## Consequences

- The metadata leak is real and accepted: an attacker with the file learns *that*
  you copied 31 characters from Bitwarden at 14:32, but not what they were.
- **Deletion is not automatic.** SQLite does not zero freed pages, so a
  `DELETE` leaves recoverable ciphertext, and WAL mode keeps content in the
  sidecar until checkpointed. Retention sweeps must run with
  `PRAGMA secure_delete = ON` and follow with `PRAGMA wal_checkpoint(TRUNCATE)`.
  "The row is deleted" and "the secret is gone" are different claims, and this
  feature needs the second one.
- A future reader reaching for SQLCipher should know it was considered and
  deferred, not overlooked.
