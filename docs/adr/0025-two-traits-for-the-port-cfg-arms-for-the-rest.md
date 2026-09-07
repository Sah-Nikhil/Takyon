---
status: accepted
---

# Two traits for the port, `cfg` arms for the rest

The macOS port is the largest piece of work left in the project
(`docs/plans/macos.md`), and the honest reading of it is that most of it will not
be started for a long time. That makes the shape of the *preparation* a decision
worth recording, because preparation that is wrong is worse than none: it costs
indirection now and gets thrown away later.

Two seams get a trait. Everything else gets a `#[cfg]` arm at the call.

- **`clips::os::ClipboardStore`** — reading and writing the system clipboard,
  synthesising the paste chord, and starting the change watcher. Four calls, one
  implementor (`WindowsClipboard`), one refusing stub off Windows.
- **`hotkey::Hotkey`**, with **`hotkey::SecondBinding`** hanging off it — register,
  rebind, and whichever extra key the platform allows. `PluginHotkey` is the only
  implementor and it is *already portable*: `tauri-plugin-global-shortcut` binds
  `Alt+Space` everywhere. The trait exists for `second_binding()`, which is
  `Some(&WindowsKeyTap)` on Windows and `None` elsewhere.

CLAUDE.md had been naming both of these as existing seams since v0.1. They did
not exist. This closes that gap rather than opening a new one.

## Considered Options

- **A trait per subsystem.** `FileIndex` and `SearchProvider` already earn theirs,
  because both have two implementations on Windows alone (walked roots vs Windows
  Search; DuckDuckGo vs Exa). Nothing else does. A trait with one implementor and
  no second one in sight is indirection that makes the Windows code harder to read
  in exchange for a macOS port that may never be written.
- **`cfg` arms everywhere, no traits at all.** This is what the codebase mostly
  does already and it works well for a leaf function — `launch::copy_to_clipboard`
  and `superkey::arm` both had a `#[cfg(not(windows))]` twin before this. It fails
  where the *set* of calls is the interesting thing. "What does a clipboard have to
  do?" was answerable only by grepping for `windows::` across four files, and the
  answer disagreed with itself: `launch.rs` owned the write, `watch.rs` the read,
  `paste.rs` the chord.
- **Two traits, `cfg` arms elsewhere.** Chosen. The trait is the unit of *porting
  work*, not the unit of platform difference. Where a port means "implement these
  four things together", a trait names the four. Where it means "this one function
  is different", a `cfg` arm says so at the function.

## Consequences

`clips::os::host()` and `hotkey::host()` are compile-time `cfg` choices returning
`&'static dyn`. They are not settings and there is no registry: a second clipboard
cannot be selected at runtime and nothing should ever make that possible.

Every clipboard write now goes through one path. `query.rs` called
`launch::copy_to_clipboard` directly for the calculator answer and for Copy path;
both now go through `ClipboardStore::write_text`. That is the visible behaviour
change in this commit, and it is a narrowing — there is no longer a second route
to the OS clipboard for a caller to reach for.

The default methods on `ClipboardStore` carry the safety rules rather than the
platform. `paste_back` writes, waits `FOCUS_SETTLE_MS`, then presses; `write`
matches on `ClipKind`. Both are defaulted because a macOS implementor that got
either ordering wrong would fail the way v0.5 originally failed — pasting into the
Palette's own input box.

**A third seam is identified and deliberately has no trait.** `clips::key`'s
`dpapi` is now `#[cfg(windows)]` with a refusing stub, because the macOS answer is
a Keychain item whose access control replaces both the account binding *and* the
entropy argument. That is not the same function with a different body; writing a
`SecretStore` trait against DPAPI today would shape the Keychain implementation
around the wrong idea. It waits for the port.

The `cfg` arms this adds are reasoned, not compiled: `cargo check --target
aarch64-apple-darwin` cannot run on the development machine — `libsqlite3-sys`'s
build script wants a cross `cc` and there is none. The macOS half of every arm
added here is therefore in exactly the state `superkey.rs`'s non-Windows stub has
always been in, and `identity.rs`'s new
`v0_11_data_dir_is_application_support_over_the_slug` test has never executed.
