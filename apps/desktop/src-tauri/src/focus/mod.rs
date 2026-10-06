//! Focus sessions (v0.16): block apps and websites for a set time.
//!
//! Plan `docs/plans/v0.16-focus.md`. Websites per ADR-0035, strictness per
//! ADR-0036. Terms from `CONTEXT.md` §Focus.

pub mod browser;
pub mod categories;
pub mod duration;
pub mod identity;
pub mod procs;
pub mod protected;
pub mod rules;
pub mod service;
pub mod session;
pub mod site;
pub mod store;
