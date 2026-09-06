---
status: accepted
extends: 0021
---

# The `!s` answer is Arc-shaped: sections, icons, source cards and favicons

The answer surface renders an accent headline, optional `##` sections with plain
headings, findings that each carry an icon and a label linked to their source,
inline `[Name](n)` links onto sources, a horizontal card strip under the first
group, and a numbered source list with real favicons.

The Agent produces that shape because `synth::prompt` asks for it. Nothing here
changes what leaves the machine except favicons, which are covered below.

## Why the previous surface read flat

Three faults, in order of weight.

**Hierarchy was inverted.** The question was set at 13px and 60% opacity, smaller
and dimmer than the answer it produced, while the headline was 16px — barely a
step above body text.

**One grey ramp did all the work.** Everything sat between `fg/85` and `fg/35` at
13 to 13.5px, so nothing grouped and the eye had no entry point.

**Ten equal-weight source rows outweighed a two-line answer.** The heaviest block
on screen was the bibliography, which is why the surface read as a list of links
with a note above it.

## The vocabulary is closed

Findings carry `{token}` naming an icon, from a fixed list of thirty. A closed set
rather than free text, because an Agent asked to invent a name invents one every
few answers and a name with no glyph behind it is a ragged gutter. Unknown tokens
fall back to a neutral icon, and the parser drops them rather than leaking braces
into the prose.

The list is written down three times: `FINDING_ICONS` in `FindingIcon.tsx`, the
`ICONS` set in `findings.ts`, and the prompt in `synth.rs`. The first two are
asserted equal by a test. The third is prose an Agent reads, so its only failure
mode is asking for a token that falls back — which is the designed behaviour.

## Links point at source numbers, never URLs

`[Name](3)` resolves against the list Rust already fetched. `answerText.ts`
refuses any target that is not a small integer, so a model that invents an address
cannot put one on screen and a click can only ever open a page `!s` retrieved.
This is the same reasoning as `docs/tbd/v0.9.md` §9: answer text is written by a
model that has just read the open web, so it is content and never markup.

## Favicons, and the one they are not

Icons come from the hosts themselves: the `<link rel="icon">` in the HTML `!s`
already fetched, falling back to `/favicon.ico`. One extra request per host, to a
host the search has already contacted, so **no new party learns anything**.

Rejected: `google.com/s2/favicons` or DuckDuckGo's equivalent. One call, always
works, no parsing — and it hands that service the full list of hosts you read, on
the one feature in the product that already leaves the machine. That is the
privacy shape ADR-0002 exists to refuse, and buying it back for a 16px image is
not a trade worth making.

Also rejected: `og:image` thumbnails on the cards. The URL is free, since the HTML
is already in hand, but the images are not. They point at CDNs the search never
contacted, run 1 to 4 MB per answer against a 2.5 MB installer and a 150 MB idle
budget, arrive after the answer so cards pop in unevenly, and cache per page —
so unlike favicons the cache never pays. The screenshots this was modelled on
show favicon cards and no article imagery.

The cache is keyed by bare host, `www.` stripped. It is stripped on **both** sides
of the seam or neither: the frontend displays and requests the bare host, so a
file written with the prefix is one nothing ever reads. That was a real bug, found
by driving a real search and noticing two of six cached icons resolved to nothing.

Bytes reach the webview through `takyon-favicon://`, the same seam application
icons use. The webview never fetches anything itself, and the CSP does not let it.

## Two icon families, two jobs

**Phosphor at `duotone` weight** for the finding gutter. Never `fill`: duotone
carries a body behind its stroke, so it survives 15px on a near-black plate
without collapsing to a silhouette. A bare 1.5px stroke at that size antialiases
to mud — the same failure a 1px accent rule hit earlier, rendering grey on some
rows and cyan on others.

**Iconoir** for larger chrome, currently the card's open-in-browser affordance.

Mixing two icon families is normally a defect, and it stays one unless the split
is by role rather than by whim. The rule: fill-weight family for small semantic
icons, stroke family for larger chrome, never adjacent at the same size. Both are
permissively licensed (Phosphor MIT, Iconoir MIT), which matters while open source
versus proprietary is undecided and GPL is ruled out.

## Consequences

The stack gains two dependencies, which `CLAUDE.md` records. Both tree-shake to
the icons actually referenced.

An answer now scrolls further, and the view no longer follows the stream. Since
the headline is the answer's title, following the tail scrolled it off the top
before it could be read. It also removed a real source of flake: where the view
settled depended on how fast tokens arrived, so the screenshot of a finished
answer was never the same twice, and that test had been failing release preflights
at random for two releases.

The prompt is longer and asks for more structure, so a weaker Agent has more to
get wrong. Every part of the shape is optional in the parser: no icon, no section
and no link all render as an ordinary finding, and unrecognised lines still fall
through to paragraphs as they did at v0.9.
