# Takyon: a brief for designing a logo

Written to be handed to a designer or an image-generation agent with no other
context. Everything needed is here.

## What the product is

Takyon is a keyboard launcher for Windows. You press one hotkey, `Alt+Space`, and
a small panel appears floating over whatever you were doing. You type a few
letters. It finds and opens applications, files, and folders, does arithmetic and
unit conversion, and holds your clipboard history. You press Enter and it
vanishes. The whole interaction takes about a second.

Mac users know this pattern as Spotlight or Raycast. Windows has Raycast now too,
in beta, and it is an Electron app.

## Who uses it

Developers and power users on Windows who live on the keyboard and resent
reaching for a mouse. People who have a hotkey launcher on their Mac and find
nothing equivalent on their PC. The kind of person who notices a 200 ms delay and
is annoyed by it.

## The two things that make it different

**It is fast in a way you can measure.** The panel appears in under 50 ms and
results rank in under 30. Most launchers claim speed. This one has a number and
publishes it. A supporting fact worth knowing: results land on screen roughly 300
milliseconds before an average typist finishes a four-letter word. The answer is
already there when you look up.

**It does not talk to the internet unless you ask it to.** A plain query never
leaves the machine. No search suggestions, no telemetry, no analytics, no
prefetch. To reach outward you type an explicit command prefixed with an
exclamation mark, `!s` for a web search or `!c` to ask Claude. That boundary is
enforced in the code and it is the closest thing the product has to a principle.

## The name

Takyon is a respelling of **tachyon**, a hypothetical particle that travels
faster than light. Its strange property is not speed. It is causality. A tachyon
arrives before it departs, because faster than light means the effect precedes
the cause.

That is the idea worth designing around. Not "quick" but "already there".

One more piece of physics that has proven useful. When a charged particle exceeds
the speed of light inside a medium, water for instance, it drags a cone of blue
light behind it. That glow is called Cherenkov radiation, and it is what makes
reactor pools glow blue in photographs. It is the visible signature of something
moving too fast, and it is only visible in the dark.

The `k` in Takyon is deliberate. The same person made an app called Diktafone, a
respelt dictaphone, so the swapped consonant is a signature rather than a typo.

## Register

Cold, precise, instrument-like. Think measuring equipment, particle physics
plates, oscilloscopes, engineering drawings. Not friendly. Not playful. Not warm.

Sibling projects, for a sense of the house style: **Tesseract**, a knowledge tool
named after the four-dimensional cube because it folds many dimensions into one
system, and **Diktafone**, a voice recorder. Both names are real words, chosen
because they describe the mechanism, and both explain themselves in one sentence.
A good name here is not clever. It is accurate.

## Hard constraints

The mark has to work at 16 by 16 pixels in the Windows system tray, where it may
be rendered in greyscale. It has to hold as a large application icon. It has to
sit inside the launcher's own input field at about 17 pixels, where a search icon
would normally go.

It appears on a dark floating panel over arbitrary desktop wallpaper, on a light
documentation page, and on a solid accent colour. It needs to invert without
being redrawn, which in practice means it should be built from filled shapes or
consistent strokes rather than from careful colour relationships.

Colour is not decided. Design the mark to survive in one colour first.

## What has already been rejected, and why

I mention these because they were tried and they failed, so they are worth not
repeating.

**A chevron or prompt caret.** Every developer tool already owns this. Warp, Fig,
Hyper, and Wave all sit in that space. It reads as "terminal" and it is not
ownable.

**A ring with a line through it.** The line reads as a prohibition sign, like a
no-entry symbol. Offsetting the line so it becomes a chord rather than a diameter
fixes the read, but the form is still generic.

**A lightning bolt.** The default visual for speed, used by everything.

**A magnifying glass.** The default visual for search, used by everything.

**A play button.** Any triangle with a flat back edge becomes one, so a wedge or
cone shape needs a curved or broken back edge to avoid it.

## What is working now

The current mark is a **Cherenkov cone**. A wedge whose apex points right and
whose back edge bows inward, with a separate dot placed beyond the apex. The
wedge is the cone of light dragged behind the particle. The dot is the particle
itself, which has already outrun the wake it created. The gap between them
carries the meaning, so it never closes.

It works at 16 pixels, inverts cleanly, and no other software uses the form.

Treat it as the standard to beat rather than as a template. If something better
exists, it will most likely come from the same place: a real piece of physics
that describes arriving before you should, drawn simply enough to survive at the
size of a tray icon.

## Tone of any words that appear alongside the mark

Short and specific. "It is already there when you look up" works. "Blazing fast
productivity" does not.
