# UIAccess and code signing

Why the Palette needs a signed helper to appear over an elevated window, what
Windows demands before it will honour that, and how to satisfy it — first on a
development machine, then for real.

**Status (2026-08-25, mid v0.1/v0.2):** the helper is built and its manifest is
verified to work (an unsigned run fails with error 740, which is the proof).
`scripts/dev-sign-uiaccess.ps1` and `remove-dev-cert.ps1` are written and their
`signtool.exe` lookup bug is fixed. **Nothing has been signed yet** — no dev
certificate has been generated, the helper has never been installed to
`%ProgramFiles%\Takyon\`, and the elevated-window overlay has never been tested
end to end. That is one elevated PowerShell run away (§5) and is not blocking —
v0.2 does not touch this path. A commercial certificate is a separate, later
v1.0-ship prerequisite (§6) and has not been sourced.

---

## 1. The problem

Windows assigns every process an **integrity level**. Takyon runs at *Medium*;
anything launched "as Administrator" runs at *High*. User Interface Privilege
Isolation (UIPI) forbids a lower-integrity process from taking foreground from a
higher-integrity window, and it refuses **silently**.

Concretely: an elevated PowerShell has focus, you press `Alt+Space`, and Takyon's
`SetForegroundWindow` call is rejected. The Palette either does not appear or
appears *behind* the elevated terminal and never receives a keystroke. This is not
a bug that can be coded around; it is the OS enforcing a security boundary that
exists to stop a normal-privilege process driving an elevated one's UI.

## 2. The only sanctioned escape

An application manifest carrying:

```xml
<requestedExecutionLevel level="asInvoker" uiAccess="true" />
```

This is the mechanism screen readers, Magnifier, the on-screen keyboard and
remote-assistance tools use, and it is what PowerToys Run uses for exactly this
reason. It grants one narrow right — drive UI above your own integrity level —
and nothing else. **It is not elevation.** `level` stays `asInvoker`; asking for
`requireAdministrator` as well would put a UAC prompt in front of a launcher at
every login and buy nothing.

Windows honours the attribute only when **both** conditions hold:

1. The binary is **Authenticode-signed**, by a certificate chaining to a root the
   machine trusts.
2. The binary lives in a **secure location** — `%ProgramFiles%`,
   `%ProgramFiles(x86)%`, or `%SystemRoot%\System32` and their subdirectories.
   These are directories a standard user cannot write to.

Fail either and the process refuses to start with `ERROR_ELEVATION_REQUIRED`
(740). There is no developer-mode exemption, no registry override and no way to
test it without satisfying both.

Condition 2 is why **portable mode is impossible** for this product. A launcher
run from a USB stick or a Downloads folder can never satisfy it.

## 3. Why a separate executable

The helper is `apps/desktop/src-tauri/uiaccess/`, a binary that does one thing.
The main app is not manifested for `uiAccess`, deliberately:

- A `uiAccess` process pays real costs. Drag-and-drop from Explorer breaks on the
  integrity mismatch, for one.
- Running the entire WebView2 surface at a raised privilege to solve a foreground
  problem is a bad trade. The helper is a few hundred lines with one syscall in
  it; the app is a browser engine.
- It confines the signing requirement. Exactly one binary in the product is
  signing-critical.

### The protocol, and its threat model

The helper listens on `\\.\pipe\com.v3sper.launcher.uiaccess` and accepts eight
bytes: a window handle. It calls `SetForegroundWindow` on it and nothing else.

The pipe's default DACL admits other processes running as the same user.
Tightening it would not buy much — code already running as you can do worse than
move a window. **The control that matters is the ownership check:** the helper
records its parent process id at startup and acts on a handle only if
`GetWindowThreadProcessId` says that process owns it. So the worst an
unauthorised caller achieves is foregrounding Takyon's own Palette, which they
could also do by pressing `Alt+Space`.

The helper also exits when its parent does, so a privileged listener is never
left running after the app it serves is gone.

## 4. How the manifest gets embedded

Through the MSVC linker, in `uiaccess/build.rs`, and the split between two options
is not cosmetic:

| Option | Carries |
|---|---|
| `/MANIFESTUAC:level='asInvoker' uiAccess='true'` | the requested execution level |
| `/MANIFESTINPUT:app.manifest` | assembly identity, supported-OS list |

`/MANIFEST:EMBED` makes the linker author its own manifest fragment with
`uiAccess="false"`, and `mt.exe` refuses to merge two fragments that disagree:

```
manifest authoring error c1010001: Values of attribute "uiAccess" not equal in
different manifest snippets.
LINK : fatal error LNK1327: failure during running mt.exe
```

So the attribute is set by `/MANIFESTUAC` and nowhere else. Putting a
`<trustInfo>` block back into `app.manifest` fails the link, which is at least the
honest place to fail.

**A useful consequence:** once the manifest is embedded, an unsigned helper cannot
be executed at all. That is why `uiaccess/Cargo.toml` sets `test = false` — a
Cargo-generated test harness for that binary fails to launch with error 740 and
takes `cargo test --workspace` down with it. Seeing 740 is confirmation the
manifest is working, not a fault.

## 5. Development: self-signing

A self-signed certificate satisfies condition 1 **for machines that trust it**.
That is enough to build and verify the feature; it does nothing for anyone else.

```powershell
# From an ELEVATED PowerShell, at the repo root:
.\scripts\dev-sign-uiaccess.ps1
```

It builds the helper in release, creates a `CN=Takyon Dev Signing` code-signing
certificate if one does not exist, installs it into `LocalMachine\Root`, signs the
helper, and copies it to `%ProgramFiles%\Takyon\`.

> **This installs a root certificate.** Until it is removed, the machine trusts
> anything signed with that key. Run `.\scripts\remove-dev-cert.ps1` when done.
> Requires the Windows SDK for `signtool.exe`.

Then point a development build at it:

```powershell
$env:TAKYON_UIACCESS_HELPER = "$env:ProgramFiles\Takyon\takyon-uiaccess-helper.exe"
bun run dev
```

**Verify it, because a silent no-op is the failure mode:**

1. Open an elevated terminal and click it so it has focus.
2. Press `Alt+Space`.
3. The Palette must appear *in front of it* and accept typing.

Without the helper, step 3 fails and nothing is logged. That is UIPI, not a bug —
which is precisely why this needs checking by hand rather than being assumed.

## 6. Shipping: a real certificate

Unresolved, and a v1.0 blocker rather than a v0.1 one. The options, with what they
cost:

| Option | Cost | Requires |
|---|---|---|
| **Azure Trusted Signing** | ~$10/month | A verified organisation, or an individual with three years of verifiable identity history. Cheapest by a wide margin. |
| **OV code-signing certificate** | ~$200–400/year | Since mid-2023 the CA/Browser Forum requires the private key on a hardware token or HSM, so there is shipping and a physical device involved. |
| **EV code-signing certificate** | more | Same hardware requirement; buys immediate SmartScreen reputation, which OV accrues over time. |

Two things to carry forward when this is decided:

- **SmartScreen reputation accrues per signed binary**, so it restarts if the
  executable is renamed. ADR-0011's separation of identity from display name is
  what keeps a future rename from costing that.
- The signing step belongs in the release pipeline, reading the certificate from a
  gitignored `signing.secrets.ps1`. The `.gitignore` already excludes `*.pfx`,
  `*.p12` and that filename.

## 7. What is not done

- The helper is **not** a Tauri bundle resource. It is only useful signed and
  installed in a trusted location, which is a step outside the normal build, so
  `scripts/dev-sign-uiaccess.ps1` places it. A build that skipped that step has no
  helper rather than shipping one Windows will refuse to start.
- The NSIS installer does not yet install it. That belongs with the real signing
  work at v1.0, since installing an unsigned helper accomplishes nothing.
