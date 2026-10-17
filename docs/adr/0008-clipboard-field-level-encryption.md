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

## Amendment (v0.14): image clips

The accepted metadata leak now includes the presence, dimensions, byte length,
source and time of image clips. An attacker with the database and blob directory
can see that you copied 31 characters from Bitwarden at 14:32 **and every
screenshot you took is in there**, even though the pixels remain encrypted.
Screenshots can contain a 2FA code, a bank page or a photographed passport, so
this raises the value of the stolen files without changing the encryption rule.

On Windows, the full PNG is stored as a nonce followed by AES-256-GCM ciphertext
in a separate blob file; the thumbnail stays encrypted in the row. Windows'
`ExcludeClipboardContentFromMonitorProcessing` marker and macOS's
`org.nspasteboard.ConcealedType` marker apply to image clips as they do to text.
The macOS image capture path remains unbuilt. The `takyon-clip` webview scheme
decrypts an image per request and sends `Cache-Control: no-store` so WebView2
does not cache the screenshot on disk.

Deleting an image clip overwrites its blob with zeros, syncs it, then unlinks it.
Overwrite on an SSD or other flash storage cannot guarantee physical erasure of
old blocks; the file's bytes were encrypted before they reached storage.
