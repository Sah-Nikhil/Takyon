# Takyon, hackathon material

Not part of the doc taxonomy in `CLAUDE.md`. Safe to delete after submission.

Angle: Takyon is the fastest way to reach anything on your machine, and the
fastest way to reach past it. One keystroke, one box, everything behind it.

---

## 1. What does it solve?

Everything on a Windows machine lives behind a different door. Start menu for
apps. Explorer for files. A shortcut nobody remembers for clipboard history. A
calculator app for one sum. A browser tab for a question. A terminal for the AI
CLI you already pay for.

Takyon is one key that opens all of them. Press `Alt+Space`, type, press Enter.
Applications, files, clipboard history, math, Windows settings pages, Steam and
Epic games, all in the same list, first answer on screen in under 30
milliseconds. It ranks by what you actually open, so by the second keystroke the
thing you wanted is already first.

Then it reaches past your desktop. `!s` answers a live question from the web and
shows you the sources it read. `!c` hands the question to the AI CLI you are
already signed in to. Same box, same keystroke, no tab, no terminal, no losing
your place.

The whole point is time. Ten seconds of hunting, forty times a day, is where your
attention goes. Takyon gives it back.

---

## 2. What issues did you face, and how did you solve them?

**Fast and useful pull in opposite directions.** Every feature wants to run when
you press the key, and the budget is 50 milliseconds. Fixed by never building the
window: it is created once at login, hidden, kept warm, and its memory trimmed
while it sits there. Showing it allocates nothing. First pixel measures 22.6 ms,
idle cost 107 MB.

**A green test suite that was lying.** The screenshot tests allowed 1% of pixels
to differ, which on this window is 5,702 pixels. Enough to hide two missing rows
and a wrong version number across two releases. Swapped the percentage for a flat
150 pixel budget and proved it by restoring an old baseline, which now fails by
1,020 pixels. Separately, the Settings window had never rendered in a real build,
because creating a window from the main thread deadlocks Tauri. Both were found
by driving the installed product instead of trusting a clean compile.

**AI features usually mean an account, a key and a bill.** Takyon has none. It
drives the agent CLIs you already have installed and signed in, Claude Code,
Codex or opencode, as a subprocess. You rank them, and the first one switched on
answers. That same ranking gives `!s` its writer for free, so web search works on
a machine that only has Codex.

**Web search wanted to double the size of the app.** A Rust HTTPS client would
have added roughly 2 MB of TLS to a 2.5 MB product. Windows already ships an HTTP
stack with TLS, the certificate store and your proxy settings, so Takyon calls
that instead. The installer did not grow at all.

**The first answers read like essays.** Nobody wants five paragraphs to learn a
score. Rebuilt `!s` on Arc Search's shape: it names the pages it is reading, then
gives a headline and a few labelled one line findings, each ending in the sources
behind it, with a line of its own wherever the sources disagree.

---

# 3-minute pitch script

Rules for this cut. The demo is the pitch, so keep talking to a minimum. No line
is longer than fifteen words. Nothing is explained twice, once on screen and once
out loud. Total spoken is about 130 words, which is well under a minute, and the
rest of the three minutes is the app doing things.

Where a beat says nothing, say nothing. Silence over a working demo reads as
confidence.

| Time | On screen | Say |
| --- | --- | --- |
| 0:00 | Empty desktop. Press the hotkey. Palette appears. | "One keystroke. Watch what fits behind it." |
| 0:10 | Type `chrome`, Enter. Chrome opens. | "Apps." |
| 0:18 | Hotkey, `2+2*3`, answer in the row. | "Math." |
| 0:26 | Hotkey, `report`, Enter, the file opens. | "Files." |
| 0:34 | Hotkey, `bluetooth`, Enter, the Windows page opens. | "Windows settings, without digging through menus." |
| 0:42 | Type one letter. The app you use is already first. | "It learns what you open. One letter is usually enough." |
| 0:52 | `Ctrl+K` on a result, then reveal in Explorer. | "Enter opens it. Ctrl+K does everything else." |
| 1:04 | `!v`, filter the clipboard list, Enter to paste. | "Bang v is everything you've copied." |
| 1:20 | `!s who won the last f1 race`, Enter. Let it run. | "Bang s searches the web." |
| 1:30 | Hosts appear, then headline, then findings. **Say nothing.** | (silence) |
| 1:50 | Click a citation. The real page opens. | "Every line cites its source. Click it, there's the page." |
| 2:05 | `!c when is the next total solar eclipse`, answer streams. | "Bang c asks an AI agent." |
| 2:20 | Settings, Agents. The ranked list and switches. | "No account, no API key. It runs the CLI you already pay for." |
| 2:35 | The four numbers, then the installer size in Explorer. | "Opens in 22 milliseconds. The installer is 2.5 megabytes." |
| 2:50 | The mark, `Alt+Space`, the repo URL. | "Takyon. One key, everything behind it." |

## The three lines that have to survive

If you fluff everything else, land these:

1. "One keystroke. Watch what fits behind it."
2. "No account, no API key. It runs the CLI you already pay for."
3. "Takyon. One key, everything behind it."

## Recording notes

- Record the demo first, then talk over it. Writing the voiceover first is what
  makes a pitch wordy.
- Turn animations off in Settings, or the idle beat fights your cuts.
- `Alt+Space` collides with PowerToys Run and with Raycast. Rebind first, or the
  video shows the wrong window opening.
- Record `!s` and `!c` live, with a real Brave key and a signed in agent. If a
  judge asks you to run it again, you want it to work.
- Put the query you typed as a caption when it flies past too fast to read.
- Keep every clip under twelve seconds.

## If you run long

Cut in this order: the `Ctrl+K` beat, the `!v` beat, then the numbers beat. Never
cut the four in a row at 0:10 to 0:42, or the `!s` citations.
