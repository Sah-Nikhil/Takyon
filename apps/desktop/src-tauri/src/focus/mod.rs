//! Focus sessions (v0.16): block apps and websites for a set time.
//!
//! Plan `docs/plans/v0.16-focus.md`. Websites per ADR-0035, strictness per
//! ADR-0036. Terms from `CONTEXT.md` §Focus.

pub mod duration;
pub mod identity;
pub mod protected;
pub mod session;
pub mod site;
