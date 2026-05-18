//! `CLI > env > TOML > default` priority chain tests.
//! Split into themed sub-files (bead axon_rust-2j9.6):
//!   * `ask`            — [ask] + [search.hybrid] knobs
//!   * `tei`            — [tei] knobs
//!   * `workers_search` — [workers] + [search] knobs
//!
//! Test BODIES are unchanged from the previous flat layout.

#[path = "priority_chain/ask.rs"]
mod ask;
#[path = "priority_chain/tei.rs"]
mod tei;
#[path = "priority_chain/workers_search.rs"]
mod workers_search;
