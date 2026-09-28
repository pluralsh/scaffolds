//! Cloud-agnostic building blocks shared by every operational function.
//!
//! Each function receives a [`Request`] (the tool input sent by a Plural workbench) and
//! answers with a [`Response`]. Requests default to [`Action::Plan`], a dry run that only
//! evaluates [`Guard`]s, so nothing is changed unless the caller explicitly asks to execute.

mod envelope;
mod error;
mod guard;

pub use envelope::{Action, Outcome, Request, Response};
pub use error::Error;
pub use guard::Guard;
