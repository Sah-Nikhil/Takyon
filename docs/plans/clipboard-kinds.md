# Clipboard history learns three kinds

Today `ClipKind` has one variant, `Text`, on both platforms. This phase adds
**`Image`** and **`Files`**, and classifies links as a property of a text clip
rather than a fourth kind.

**Both platforms, one phase, after the macOS port.** Not inside it: the port is
already the largest piece of work in the project, and this would turn row 6 from
"port a watcher" into "design a blob store, a preview surface, a size policy and
an encryption amendment" on two operating systems at once. Not macOS-first
either — that would make macOS the reference implementation for a feature Windows
has to match, inverting every other decision in the port.

`paste.rs` was written expecting this. Its `ClipKind` match exists precisely so a
second variant is an addition rather than a refactor, and its doc-comment already
names "images and file lists" as the pair to expect.

## Read first

[ADR-0006](../adr/0006-clipboard-history-storage-and-safety.md) and
[ADR-0008](../adr/0008-clipboard-field-level-encryption.md) — this phase amends
both. [ADR-0030](../adr/0030-the-macos-clipboard.md) for the macOS mechanics.
`clips/store.rs`, `clips/watch.rs`, `clips/paste.rs`.

## The threat model changes, and ADR-0008 must say so

ADR-0008 accepts a specific leak: "an attacker with the file learns *that* you
copied 31 characters from Bitwarden at 14:32, but not what they were."

With images that sentence becomes **"…and every screenshot you took is in
there."** People copy screenshots of things they would never copy as text — a
2FA code on screen, a bank page, a photographed passport. The encryption is
unchanged and still sound; what changes is the value of the file to whoever gets
it. ADR-0008 gets an amendment that says this plainly rather than inheriting a
sentence written when only text was possible.

The mechanism does **not** get weaker. The exclusion markers are properties of the
clipboard, not of the text: Windows' `ExcludeClipboardContentFromMonitorProcessing`
and macOS's `org.nspasteboard.ConcealedType` (ADR-0030) both apply to an image
clip exactly as they apply to a text one.

## The three kinds

| kind | what is stored | what is not |
|---|---|---|
| `Text` | the string, encrypted, capped at `MAX_CHARS` (4 MB) | — |
| `Image` | an encrypted blob file + an encrypted thumbnail in the row | — |
| `Files` | the **paths**, encrypted, as a list | the file contents, ever |

**`Files` stores paths and nothing else.** Finder and Explorer both put
*references* on the clipboard, so copying a 40 GB folder is one string of a couple
of hundred bytes. There is no size problem here and there was never going to be
one.

Its real cost is the opposite: paste a file clip back a week later and the path
may be dead. That is exactly how Windows behaves with a file drop today, and the
right answer is to show it — a `Files` row whose paths no longer exist is drawn
dimmed, and paste-back reports which are missing rather than silently pasting a
shorter list.

**Links are a classification of `Text`, not a kind.** A copied URL is a string on
both platforms. Detection reuses `search::fetch::parse_url`, which already refuses
anything that is not `http(s)`; a classified link row gets the host as its
subtitle, a favicon through `fetch::get_icon` (ADR-0022's cache), and **Open** as
an additional action.

## Where image bytes live

Not in `clips.db`. Encrypted blob files under the data directory, with the row
holding a pointer, a nonce, and a small encrypted thumbnail for the list.

```
%LOCALAPPDATA%\v3sper\takyon\clips\<clip-id>.bin      (Windows)
~/Library/Application Support/com.v3sper.takyon/clips/<clip-id>.bin   (macOS)
```

Two reasons, and the second is the one that matters:

1. A Retina screenshot is routinely 5–20 MB. Inline BLOBs would take `clips.db`
   into the gigabytes, and every retention sweep would rewrite it.
2. **Secure deletion becomes an explicit operation on a named file** rather than a
   property of SQLite's page allocator. ADR-0008 already warns that "the row is
   deleted" and "the secret is gone" are different claims; a blob file makes the
   fix obvious — overwrite, then unlink — instead of hoping `secure_delete` and a
   WAL checkpoint covered it.

**Cap: 32 MB.** Past it the copy is skipped rather than truncated, matching
`MAX_CHARS`'s existing rule — a clip that is silently half a screenshot is worse
than one that is absent.

**Thumbnail: 256 px on the long edge**, PNG, encrypted, stored in the row. The
Palette never reads a full blob to draw a list.

## Retention and pruning

Blobs must participate in every path that destroys rows, or the database shrinks
while the directory grows forever.

- **The retention sweep** (`store::sweep`, already running on its own thread with
  the user's 1 day / 1 week / 1 month / forever setting) deletes the blob for every
  row it removes — overwrite, then unlink, then the row.
- **Clear history** (`clip_clear`, already an IPC command and a Settings button)
  does the same for every row, then removes any orphan file left in `clips/`.
- **An orphan sweep on startup.** A crash between unlink and commit, or the
  reverse, leaves a file with no row or a row with no file. Both are reconciled
  once at startup: files with no row are deleted, rows with no file are deleted.
- **A size ceiling, reported.** Settings → Clipboard shows what the history
  currently occupies on disk. Without a number nobody prunes, and "why is my disk
  full" is not a question this feature should ever cause.

## Capture, per platform

**Windows.** `watch.rs` gains `CF_DIB` / `CF_DIBV5` and `CF_HDROP` alongside
`CF_UNICODETEXT`, in that priority order — an application that offers both an
image and its text description is offering the image as the payload. `CF_HDROP` is
a path list; decode it and store the paths.

**macOS.** `NSPasteboard` types `public.png` / `public.tiff` and `public.file-url`,
read through the same `changeCount` poll ADR-0030 establishes. `NSPasteboard`
gives typed reads, so this is a type-priority list rather than format enumeration.

Both go behind `ClipboardStore` (ADR-0025), which grows one method:

```rust
/// What is on the clipboard now, as a kind and its payload.
fn read(&self) -> Option<Captured>;   // replaces read_text
```

`read_text` stays as a defaulted convenience for the `Text` case, so `query.rs`'s
Copy paths are untouched.

## Paste-back

`Paste` already carries a `kind`, and `ClipboardStore::write` already matches on
it — that match gains two arms.

- `Image`: decrypt the blob, write the image type back to the clipboard.
- `Files`: write the path list back as `CF_HDROP` / `public.file-url`, so pasting
  into Explorer or Finder performs a real file copy.

The chord is unchanged, and so is the ordering rule that makes it safe: the
clipboard write happens first, so a refused keystroke — a UIPI boundary on
Windows, a missing Accessibility permission on macOS (ADR-0030) — leaves the user
one manual paste away rather than with nothing.

## The Palette

An image row draws its thumbnail in the icon slot at the row height, with
dimensions and byte size as the subtitle. A files row draws the system icon of the
first path, with the count and the common parent directory as the subtitle. A
classified link row draws its favicon.

`!v`'s existing preview pane shows the full image, decrypted on demand and never
cached to disk.

## Tasks

1. **`ClipKind` grows two variants**, and the schema migration that goes with it —
   `kind`, `blob_path`, `blob_nonce`, `thumb`, `byte_len`.
2. **Blob store**: write, read, overwrite-and-unlink, orphan reconciliation. Its
   own module, `clips/blobs.rs`, because every path that destroys data goes
   through it and they should be in one file.
3. **`ClipboardStore::read`** and the two platform capture paths.
4. **Retention, clear, and the startup orphan sweep** wired to the blob store.
5. **Paste-back arms** for both new kinds.
6. **Link classification** on `Text`, with the favicon and the Open action.
7. **Palette rows**, thumbnail rendering, and the `!v` preview.
8. **Settings → Clipboard**: on-disk size, and the existing retention control
   documented as covering blobs.
9. **ADR-0008 amendment** — the threat-model paragraph above.

## Exit criteria

Copy a screenshot, summon `!v`, see it, paste it back into another application
and get the image. Set retention to one day, wait, and confirm both the row and
its blob file are gone. Copy a password from a password manager and confirm
nothing is recorded, on both platforms.
