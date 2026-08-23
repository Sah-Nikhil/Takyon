---
status: accepted
---

# Clipboard history is encrypted at rest and never appears in Bangless results

Clipboard history inevitably captures secrets — every password copied out of a
password manager passes through it. Three decisions follow. The history is stored
in SQLite encrypted at rest, with the database key protected by Windows DPAPI and
bound to the user account, so another user on the same machine cannot read it.
Retention is the user's choice from a fixed list (forever, 6 months, 1 month,
1 week, 1 day), with entries older than the chosen window deleted, not hidden.
And clipboard Entries never surface in Bangless results — they are reachable only
through their own Bang and the dedicated history view.

Captures are skipped when the source application sets the
`ExcludeClipboardContentFromMonitorProcessing` clipboard format, which password
managers already use, and additionally when the foreground application appears on
a user-editable blocklist.

## Consequences

Excluding clipboard Entries from Bangless costs a little convenience and removes
the entire shoulder-surfing class of problem: a result list glanced at over a
shoulder can never contain a secret. Honouring the exclusion format is not
sufficient on its own — not every application sets it — which is why the blocklist
exists alongside it, and why it is user-editable rather than fixed: a user who
deliberately wants to keep copied passwords in history can allow it.
