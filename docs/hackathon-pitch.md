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

Two things to keep straight while recording. What you say is below, in order.
What is on screen while you say it is the beat table after it.

About 400 spoken words across three minutes. That is slow, with gaps. Every
sentence is short on purpose, so read them as written and let the demo fill the
space between them.

## What to say

**0:00, the problem.** Two sentences, over an empty desktop.

> Everything on your computer lives somewhere different. Start menu for apps,
> Explorer for files, a browser tab for a question, a terminal for the AI you pay
> for.
>
> I wanted one key for all of it.

**0:15, the demo.** Press the hotkey and start typing. Narrate lightly, one line
per thing, and let each one land before the next.

> This is Takyon. One shortcut, from anywhere in Windows.
>
> Apps. Math. Files. Windows settings pages, which normally take three clicks
> each.
>
> It ranks by what you actually open, so after a few days one letter is usually
> enough. Enter opens it, Ctrl+K does everything else. Reveal it, copy the path,
> run it as administrator.
>
> Bang v is everything you have copied, searchable.

**1:20, the part that is not a launcher.**

> Now the part I care about. Bang s searches the web.
>
> It asks Brave, then reads the actual pages, and comes back with a headline and
> four lines. Every line cites where it came from, and the citation opens the
> page. If the sources disagree, it says so instead of quietly picking one.
>
> That is a live search. Nothing is cached for this video.

**2:00, the differentiator.** This is the bit judges will ask about, so slow
down.

> Bang c asks an AI agent directly, and a follow up keeps going in the same
> window.
>
> Here is what is different. Takyon has no AI account and no API key of its own.
> It drives the CLI you already installed and signed in to. Claude Code, Codex,
> opencode. You rank them, and the first one switched on answers.
>
> So there is nothing new to subscribe to, and the same ranking is what writes
> the web search answer.

**2:35, why it is real.**

> It opens in 22 milliseconds and sits at 107 megabytes. The installer is two and
> a half.
>
> Web search needed HTTPS, and a Rust client would have doubled that installer,
> so it calls the HTTP stack Windows already ships. It did not grow at all.
>
> Nine versions built, four layers of tests.

**2:50, close.**

> Takyon. One key, everything behind it.

## What is on screen while you say it

| Time | On screen |
| --- | --- |
| 0:00 | Empty desktop. Press the hotkey. Palette appears on "one key for all of it" |
| 0:15 | Type `chrome`, Enter. Chrome opens |
| 0:25 | Hotkey, `2+2*3`, the answer in the row |
| 0:32 | Hotkey, `report`, Enter, the file opens |
| 0:40 | Hotkey, `bluetooth`, Enter, the Windows page opens |
| 0:52 | One letter, and the app you use is already first |
| 1:00 | `Ctrl+K`, then reveal in Explorer |
| 1:10 | `!v`, filter the list, Enter to paste |
| 1:20 | `!s who won the last f1 race`, Enter |
| 1:30 | Hosts being read, then headline, then findings. Let this run |
| 1:50 | Click a citation, the real page opens |
| 2:00 | `!c when is the next total solar eclipse`, the answer streams in |
| 2:10 | A follow up, in the same window |
| 2:20 | Settings, Agents: the ranked list, the switches, the locked model |
| 2:35 | The four numbers on a card |
| 2:45 | The installer in Explorer with its size visible |
| 2:50 | The mark, `Alt+Space`, the repo URL |

## The three lines that have to survive

If you fluff everything else, land these:

1. "I wanted one key for all of it."
2. "No AI account, no API key. It drives the CLI you already pay for."
3. "Takyon. One key, everything behind it."

## Recording notes

- Record the demo first, then talk over it. Writing the voiceover first is what
  makes a pitch wordy.
- Turn animations off in Settings, or the idle beat fights your cuts.
- `Alt+Space` collides with PowerToys Run and with Raycast. Rebind first, or the
  video shows the wrong window opening.
- Record `!s` and `!c` live, with a real Brave key and a signed in agent. If a
  judge asks you to run it again, you want it to work.
- Caption the query when it goes past too fast to read.
- Keep every clip under twelve seconds.

## If you run long

Cut in this order: the `Ctrl+K` line, the `!v` line, then the WinHTTP sentence at
2:35. Never cut the no-account paragraph at 2:00, or the `!s` citations.
