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

Total 3:00, times cumulative. Record at 1920x1080. Capture the Palette at 150%
scaling so the text survives being scaled down. Every clip is real, no mockups.

## 0:00 to 0:15, the hook

**On screen:** a plain Windows desktop. Nothing open.

> Opening an app. Finding a file. Checking something you copied ten minutes ago.
> One sum. One question. Five different places on your computer.

**Press the hotkey. The Palette appears mid-sentence.**

> Or one.

## 0:15 to 0:50, everything on your machine

**One take, no cuts.** Type and Enter each of these, roughly four seconds apart:

1. `chrome`, Enter. Chrome opens.
2. Hotkey, `2+2*3`, the answer sits in the row.
3. Hotkey, `report`, a file appears with its real icon. Enter opens it.
4. Hotkey, `bluetooth`, the Windows Bluetooth page. Enter.

> An application, a calculation, a file, a Windows settings page. Same box, same
> keystroke, no menu, no Explorer window. The first result is on screen in about
> twenty milliseconds, which is faster than you can watch it happen.

## 0:50 to 1:20, it learns, and it does more than open

**Screen recording:** type a single letter and show the top row being the app you
actually use. Then press `Ctrl+K` on a result.

> It ranks by what you actually open, so one letter is usually enough.

**On screen:** the action menu, then reveal in Explorer.

> Enter is not the only thing you can do. Reveal it, copy its path, run it as
> administrator, without touching the mouse.

**Screen recording:** hotkey, `!v`, the clipboard history list, filter by type,
Enter to paste.

> Bang v is everything you have copied, searchable, pasted back where you were.

## 1:20 to 2:05, past your machine

**Screen recording:** type `!s who won the last f1 race`, Enter. Let it run live.

> One character reaches the web instead.

**On screen:** the header turns amber and says the query left the machine. The
hosts being read appear. Then the headline, then the findings, each ending in
numbered chips.

> It searches, reads the actual pages, and gives you a headline and four lines,
> each one citing where it came from. Not ten blue links, and not an essay.

**Screen recording:** click citation `[2]`, the real page opens in the browser.

> Every number is the page behind it, one click away. Where the sources disagree,
> it says so instead of quietly picking one.

## 2:05 to 2:30, the AI you already pay for

**Screen recording:** `!c when is the next total solar eclipse`, Enter. The answer
streams in. Type a follow up in the same window.

> Bang c asks an agent directly, and a follow up keeps going in the same window.

**On screen:** Settings, Agents page. Show the ranked list and the switches.

> Takyon has no AI account and no API key. It drives Claude Code, Codex or
> opencode, whichever you already have signed in, and you decide the order.
> Nothing new to sign up for, nothing extra to pay.

## 2:30 to 2:45, the numbers

**On screen:** the bench output, or the four numbers on a card, then the
installer in Explorer with its size visible.

> Hotkey to first pixel, 22.6 milliseconds. Login to responsive, 311. Idle
> memory, 107 megabytes. The installer is two and a half.

## 2:45 to 3:00, close

**On camera, or over the Palette sitting open and empty.**

> Nine versions. Apps, files, clipboard, calculator, ranking that learns, agents,
> and web search. Four layers of tests, because a launcher cannot be checked by
> one.
>
> Takyon. One key, everything behind it.

**Final frame:** the mark, the repo URL, `Alt+Space`.

---

## Shot list, in recording order

1. Empty desktop, then the Palette appearing on the hotkey.
2. The four in a row take: app, calculation, file, settings page.
3. One letter, correct top row. Frecency doing its job.
4. `Ctrl+K` action menu, reveal in Explorer.
5. `!v` clipboard history, filtered, pasted.
6. `!s` live: header, hosts being read, headline, findings, citations.
7. Clicking a citation, the real page opening.
8. `!c` answer, then a follow up in the same window.
9. Settings, Agents: ranked list, switches, model and effort dropdowns.
10. Bench numbers, and the installer size in Explorer.

## Recording notes

- Turn animations off in Settings before recording, or the idle beat fights your
  cuts.
- `Alt+Space` collides with PowerToys Run and with Raycast. Rebind before
  recording, or the video shows the wrong window opening.
- Record `!s` and `!c` live, with a real Brave key and a signed in agent. If a
  judge asks you to run it again, you want it to work.
- Keep every clip under twelve seconds.

## What to cut if you run long

In this order: the `Ctrl+K` shot, the `!v` shot, then the follow up half of the
`!c` shot. Never cut the four in a row take or the `!s` citations.
