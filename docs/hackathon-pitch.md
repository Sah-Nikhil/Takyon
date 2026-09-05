# Takyon, 3-minute pitch

Not part of the doc taxonomy in `CLAUDE.md`. Hackathon material, safe to delete.

Total 3:00. Times are cumulative. Record at 1920x1080, capture the Palette at
150% scaling so text is legible when the video is scaled down.

---

## 0:00 to 0:20, the problem, on camera or voiceover over a black frame

> Every launcher that answers questions sends your keystrokes to a server.
> Spotlight does. Raycast does. The moment a launcher gets useful, it gets
> chatty.
>
> Takyon doesn't. And I can show you.

**On screen:** Takyon's mark, then cut straight to the demo. No title card longer
than two seconds.

---

## 0:20 to 0:50, the guarantee, and the proof

**Screen recording, one take:** Resource Monitor open on the right filtered to
`takyon.exe`, the Network tab visible. Press the hotkey. Type `chrome`, then
`2+2`, then `report.pdf`, slowly.

> Applications, a calculation, a file. All of it local, all of it under 30
> milliseconds. Watch the network graph while I type.

**On screen:** the connection count stays at zero. Hold on it for two full
seconds. This is the shot the whole pitch rests on, so don't rush it.

> Nothing. Not a suggestion request, not telemetry, not a prefetch. A line
> without a bang never touches the network, and that's a correctness rule in the
> codebase, not a preference.

---

## 0:50 to 1:35, the bangs, where it does leave

**Screen recording:** type `!s who won the last f1 race`, press Enter.

> One character changes that. Bang-s searches the web.

**On screen:** the header turns amber and reads "Left this machine, Brave
Search". Let the phases show: searching, reading sources, then the answer
streaming in with `[1]` `[2]` citations and the source list underneath.

> The colour is the tell. Everything contained is cool, everything outbound is
> warm, so you can see the boundary rather than trust it.
>
> Brave returns the pages, Takyon reads them over the HTTP stack Windows already
> ships, and then it hands the text to an agent to answer from.

**Screen recording:** click source `[1]`, browser opens the real page. Cut back.

**Screen recording:** type `!c when is the next total solar eclipse`, Enter.

> Bang-c asks an agent directly. Claude Code, Codex or opencode, whichever you
> already have installed and signed in to.

---

## 1:35 to 2:05, the part judges will ask about

**On camera, or over the Settings, Agents page:**

> Takyon holds no LLM account. No API key of its own, no subscription, no
> proxy in the middle. It runs the CLI you already pay for, as a subprocess, and
> reads its output.
>
> That means signing in never happens in my app. It happens in Claude's own CLI,
> where it already did.

**On screen:** the Agents page. Show the ranked list, a switch per agent, the
locked model and effort dropdowns.

> You rank them. Bang-c asks the first one that's switched on, so a signed-out
> agent gets stepped over instead of being a dead end. And bang-s uses that same
> ranking to write its answer, which means web search works on a machine that has
> only Codex installed.

**On screen:** Settings, Web Search page. Point at the key field.

> The one key it does hold, yours for Brave, is wrapped with DPAPI for your
> Windows account and never sent back to the interface. Settings shows you four
> characters of it.

---

## 2:05 to 2:35, the engineering claim

**On screen:** a terminal running `bun run bench`, or the numbers as plain text
over the Palette.

> Hotkey to first pixel: 22.6 milliseconds against a 50 millisecond budget.
> Login to responsive: 311. Idle memory: 107 megabytes.
>
> The whole installer is two and a half megabytes.

**On screen:** File Explorer showing `Takyon_0.9.0_x64-setup.exe`, 2.5 MB.

> Web search needed HTTPS. The obvious move was a Rust HTTP client, which would
> have added about two megabytes of TLS to a two-and-a-half megabyte product. So
> it calls WinHTTP instead. The OS already has TLS, a certificate store and your
> proxy settings. The installer didn't grow at all.

---

## 2:35 to 3:00, close

**On camera:**

> Nine phases built. Applications, files, clipboard history, a calculator,
> ranking that learns what you actually open, agents, and now web search. 562
> Rust tests, 98 screenshot tests, four test layers because a launcher can't be
> verified by one.
>
> What's left is a code-signing certificate, so it can appear over elevated
> windows, and a week of somebody who isn't me using it.
>
> Takyon. Local by default. Networked only when you say so, one character at a
> time.

**Final frame:** the mark, the repo URL, and the one line worth remembering:
"a bangless query never touches the network."

---

## Shot list, in recording order

Record these before writing any voiceover over them. Every one is real, no
mockups.

1. Resource Monitor beside the Palette, typing a bangless query, zero
   connections. **The most important shot in the video.**
2. `!s` with a live question: header, phases, streaming answer, sources.
3. Clicking a source, browser opening the real page.
4. `!c` with a question, answer streaming, then a follow-up continuing in the
   same window.
5. Settings, Agents: the ranked list, switches, the model and effort dropdowns.
6. Settings, Web Search: the key field with a stored key showing four characters.
7. `bun run bench` output, or the numbers on a card.
8. The installer in Explorer with its size visible.
9. `Ctrl+K` action menu on a result, if there's a spare two seconds.

## Recording notes

- **Turn off animations** in Settings before recording the Palette, or the idle
  beat will fight your cuts.
- The hotkey is `Alt+Space` by default and it collides with PowerToys Run. Rebind
  to something free before recording, or the video shows the wrong window opening.
- Record `!s` with a real Brave key. Fixtures look identical on camera, and
  saying "this is live" while it isn't is the one thing that sinks a demo if
  somebody asks you to run it again.
- Keep every clip under twelve seconds. Nine shots at eight seconds each is
  seventy two seconds of footage, which is enough for a three minute cut.

## What to cut if you run long

In this order: the Ctrl+K shot, the follow-up half of shot 4, and the WinHTTP
explanation at 2:05. Never cut the Resource Monitor shot or the DPAPI sentence.
Those are the two claims nobody else in the room will be making.
