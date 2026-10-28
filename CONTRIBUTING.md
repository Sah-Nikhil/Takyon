# Contributing to Takyon

Thanks for wanting to help. Bug reports, fixes and ideas are all welcome.

Takyon is source-available under CC BY-NC-SA 4.0, not open source. Read
`NOTICE` for what that means in practice.

## Contribution terms

Every pull request includes a checkbox confirming that you agree to these
terms. A pull request is merged only once that box is ticked.

By submitting a contribution (code, documentation, tests, assets or anything
else) to this repository, you agree that:

1. **You can grant these rights.** The contribution is your own original work,
   or you have the right to submit it under these terms. If your employer has
   rights to work you create, you have its permission or it has waived those
   rights.
2. **Copyright licence.** You grant V3SPER a perpetual, worldwide,
   non-exclusive, royalty-free, irrevocable licence to use, reproduce, modify,
   prepare derivative works of, publicly display, distribute and sublicense
   your contribution. This includes the right to license it, as part of Takyon
   or separately, under any terms, including terms other than CC BY-NC-SA 4.0.
3. **Patent licence.** You grant V3SPER, and everyone who receives Takyon,
   a perpetual, worldwide, non-exclusive, royalty-free, irrevocable patent
   licence to make, use, sell, offer to sell, import and otherwise transfer
   your contribution, alone or combined with Takyon, under any patent claims
   you can license that your contribution would infringe.
4. **You keep your copyright.** These terms are a licence, not an assignment.
   You remain free to use your own contribution in any way you like.
5. **No warranty, no obligation.** You provide the contribution as is, and
   V3SPER is not obliged to use it.
6. **Third-party material** is identified as such in the pull request, with its
   licence. It must be compatible with Takyon: no GPL, LGPL, AGPL or other
   copyleft code, which `bun run check:licences` also enforces for
   dependencies.

Your contribution is also published under CC BY-NC-SA 4.0, like the rest of
the repository.

## Before you open a pull request

Package manager is **bun**. Never npm, npx, yarn or pnpm.

```
bun install
bun run typecheck && bun run lint
bun run test
```

`bun run test` runs every layer: Rust unit and integration tests, TypeScript
tests and Playwright screenshots. All three must pass.

- Tests come first. A bug fix needs a test that failed before the fix.
- Use the vocabulary in `CONTEXT.md` exactly: Palette, Bang, Entry, Source,
  Agent and the rest.
- Comments are short. `bun run check:comments` enforces the ceiling, and
  `CLAUDE.md` explains the style.
- No colour literals outside `apps/desktop/src/theme/themes.ts` and
  `styles.css`. `bun run check:colours` enforces it.
- A Bangless query must never touch the network (ADR-0002).
- Settled decisions live in `docs/adr/`. If your change contradicts one, say
  so in the pull request instead of working around it.

## Commit messages

One short line: `<VERB> d<phase>.<n> <subject>`, where the verb is `NEW`,
`FIX` or `UPDATE`. If you are unsure of the number, leave it out and the
maintainer will set it when merging.
