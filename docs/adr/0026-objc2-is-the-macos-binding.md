---
status: accepted
---

# `objc2` is the macOS binding, and it costs no new crates

Every macOS subsystem the port needs — `NSWorkspace`, `NSPasteboard`, `NSPanel`,
`NSStatusItem`, `NSBundle`, `CGEventPost`, `URLSession` — is reached through the
**`objc2` family, declared as direct dependencies under
`[target.'cfg(target_os = "macos")'.dependencies]`**. Nothing is reached through
hand-rolled `objc_msgSend`.

`docs/plans/macos.md` treated this as a stack change requiring an ADR because it
looked like adding a dependency tree. It is not. Reading `Cargo.lock`:

| crate | version | already pulled in by |
|---|---|---|
| `objc2` | 0.6.4 | `tauri`, `wry`, `tao`, `global-hotkey`, `tray-icon`, `muda`, `rfd`, `window-vibrancy` |
| `objc2-foundation` | 0.3.2 | `tauri`, `tauri-plugin-fs`, `wry`, `tao`, `tray-icon` |
| `objc2-app-kit` | 0.3.2 | `tauri`, `tauri-runtime-wry`, `wry`, `tao`, `global-hotkey`, `tray-icon` |
| `objc2-core-foundation` | 0.3.2 | `objc2-app-kit`, `objc2-foundation`, `wry`, `muda` |
| `objc2-core-graphics` | 0.3.2 | `tray-icon`, `softbuffer`, `objc2-ui-kit` |
| `block2` | 0.6.2 | `objc2-app-kit`, `objc2-foundation`, `wry`, `tao` |
| `dispatch2` | 0.3.1 | `objc2-core-foundation`, `tao`, `rfd` |

One version of each, no duplicates, across 470 packages. Naming them directly
adds **zero crates** to the tree and zero bytes to the bundle. `objc2` is a
dependency of `tauri` itself, so it is already load-bearing whether we name it or
not.

## Considered Options

- **Hand-rolled `extern "C"` `objc_msgSend`.** Rejected. Message dispatch by hand
  in a codebase that already links a checked binding is how a memory-safety bug
  arrives in code nobody can review. This is not a size or dependency argument.
- **The older `cocoa` / `objc` crates.** Rejected: unmaintained relative to
  `objc2`, and they would be genuinely new crates, since nothing in the tree pulls
  them.
- **Shell out to command-line tools wherever one exists.** Right for a *stable OS
  interface* and wrong as a policy. `open`, `pbcopy` and `pbpaste` were the
  stopgap that got four rows of the port working without this decision; they are
  superseded by ADR-0028 and ADR-0030 respectively. There is no command-line
  stand-in for an icon bitmap, an FSEvents stream or a pasteboard change count,
  and parsing tool output for those would be worse code than the FFI it avoids.
- **`objc2` family as direct dependencies.** Chosen.

## Consequences

**The real cost is version coupling, and it is manageable.** If Takyon pins
`objc2 = "0.6"` and Tauri later moves to `0.7`, cargo resolves two copies and
Objective-C types stop being interchangeable across the boundary — an `NSString`
made by one is not an `NSString` to the other, and the error is a type error at
the seam rather than anything subtle. To keep that visible:

- Pin to the SemVer range Tauri already resolves (`objc2 = "0.6"`,
  `objc2-app-kit = "0.3"`), never to an exact patch.
- `bun run check:macos` runs `cargo tree --duplicates` and fails on a duplicate
  `objc2`. A Tauri upgrade that splits the family is then caught by the gate
  rather than discovered by a compile error in the middle of a feature.
- A Tauri upgrade and an `objc2` bump are one commit, never two.

**Licensing is clear.** The whole family is MIT/Apache-2.0, so ADR-0005's
open-question on distribution is not constrained by this.

**Two crates in this port are genuinely new**, and neither is `objc2`:

- **`security-framework`** for the Keychain (ADR-0030). MIT/Apache.
- **whatever answers FSEvents**, if the file index ever needs a watcher — which
  ADR-0027 removes the need for, so this one may never be spent.

**What this does not settle.** `objc2` being free removes the *cost* argument from
TBC-0013's HTTP-client decision; it does not by itself decide it. ADR-0029 does.
