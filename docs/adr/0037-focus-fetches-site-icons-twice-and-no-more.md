---
status: accepted
extends: 0022
---

# Focus fetches site icons twice, and no more

Focus shows each website in a Focus category with its real icon. Takyon fetches
those icons at two moments only. The first is once per machine, a minute after
first launch, for the sites the built-in Focus categories ship with. The second
is when the user adds a website to a Focus category or a Block list. Both write
into the `!s` favicon cache (`search::favicon`, ADR-0022), and both skip any
host already cached.

## The problem

The user asked for website icons on a Focus category's page, drawn the way `!s`
draws its sources. `!s` icons come from a disk cache that only a search fills,
and only from hosts that search already contacted, so ADR-0022 added no new
destination. Focus sites have never been searched. Showing their icons means
either contacting each site or shipping the icons with Takyon.

This sits against a rule the project states up front: "Bangs are the only way
anything leaves the machine" (`CLAUDE.md`). ADR-0002's own scope is narrower: a
Bangless *query* never touches the network. Nothing here runs on the query
path. The broader claim does change, and this ADR is where that change is
recorded.

## The decision

- **First launch.** A thread started in deferred init waits 60 s, so it never
  competes with the login budget. It then fetches `https://<host>/favicon.ico`
  for each built-in site, falling back to the home page's declared icon, in
  parallel. When anything arrives, or everything was already cached,
  `focus.icons_fetched` is set and the fetch never runs again. A launch that
  fetched nothing, because it was offline, tries again next launch.
- **On add.** Picking "Block website: <host>", in the start form or on a Focus
  category's page, fetches that one host's icon if it is not cached.
- **Nothing else.** Opening a page, starting a session and blocking a site never
  fetch. A host missing from the cache draws its letter tile, as `!s` does.

The destinations are the sites themselves: the hosts Focus is told to block.
Takyon calls no favicon service, for ADR-0022's reason. Such a service would
receive the user's list of distracting sites.

## Considered options

| Option | Why not |
|---|---|
| Fetch on view, like a browser | Opening Settings would contact about sixty sites unprompted, and again for each new host on every page |
| Ship the icons in the installer | Takyon would carry third-party brand marks and keep them current by release, and a site the user adds would still need a fetch or a letter tile |
| No icons, letter tiles only | What the user asked to replace |

## Consequences

- **"Only Bangs leave the machine" now has a stated exception**, scoped to these
  two moments. `CLAUDE.md` names this ADR beside that sentence.
- A firewall watcher sees about sixty `GET /favicon.ico` requests, a minute
  after the first launch. `docs/verify/v0.16.md` §G says so and expects them.
- The cache is shared, so a site searched with `!s` already has its icon in
  Focus, and the reverse holds too.
- CI's daily job runs the fetch (`cargo test --test focus -- --ignored`) beside
  the `!s` live test.
