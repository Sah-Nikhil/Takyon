---
status: accepted
---

# Time zones go through jiff, with tzdb bundled on Windows

The calculator answers time zone questions (`12 pm utc`, `3:30pm pst to ist`,
`time in tokyo`) from tzdb data through the `jiff` crate. On Windows the data is
bundled into the binary; on macOS jiff reads `/usr/share/zoneinfo`. Plan:
`docs/plans/v0.17-time-zones.md`.

## The problem

A time zone answer needs the rules for every zone: offsets, daylight saving
dates, and their history. Those rules change a few times a year as governments
change them, and the IANA tz database (tzdb) is where everyone gets them. The
question is which copy Takyon reads and through what code.

ADR-0002 is not at stake. Every option here is local data; none touches the
network, so time zones answer on the Bangless path, unlike currency.

## The decision

`jiff` 0.2 with its default features, as a direct dependency.

- It was already in `Cargo.lock`, so the dependency added one line to the lock
  and no package. ADR-0026 set that precedent for macOS crates.
- MIT OR Unlicense. Permissive, so ADR-0005's open licensing question stays open.
- One implementation for both platforms. On Windows, `jiff-tzdb-platform`
  bundles tzdb, and jiff maps the Windows zone id from
  `GetDynamicTimeZoneInformation` to IANA itself. On macOS it reads the OS copy.
- It resolves DST gaps and folds correctly (`compatible`: a gap shifts forward,
  a fold takes the earlier instant), which is the part a hand-written table
  gets wrong.
- It caches the system zone for 5 minutes, so a zone change in Windows Settings
  reaches the Palette within that, with no hook of ours.

Display names in words ("India Standard Time") are not in tzdb at all. They are
a hand-written table in `sources/calc/tz.rs`, keyed by zone and code together,
because tzdb gives `IST` to Kolkata, Dublin and Jerusalem.

## Alternatives

- **The OS's own data.** Windows ships ICU (`icu.dll`, `ucal_*`) and updates it
  through Windows Update; macOS has `NSTimeZone`. Fresh data and zero crates,
  but FFI on Windows and a second implementation for macOS. The staleness this
  would fix is the one cost of the decision, and `docs/tbc/0016` watches it.
- **Windows' registry zones** (`EnumDynamicTimeZoneInformation`). Windows names,
  not IANA, so `America/New_York` is `Eastern Standard Time` all year, and a
  city lookup would need its own mapping table.
- **`chrono-tz`.** A new crate, bundles tzdb on every platform, no system zone
  without a second crate (`iana-time-zone`).
- **Hand-rolled tzdb.** No.

## Consequences

- On Windows, Takyon's rules are as fresh as its last release. A rule change
  between releases gives wrong answers for that zone, silently. TBC-0016.
- The bundled data costs binary size; measured in the plan's § What the build
  changed.
- macOS gets time zones with no `cfg` arm, except the 12/24-hour setting, which
  reads Windows' short-time format and is 12-hour elsewhere until v0.12.
