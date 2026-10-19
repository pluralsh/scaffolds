//! Cloud-agnostic building blocks shared by every operational function.
//!
//! Each function receives a [`Request`] (the tool input sent by a Plural workbench) and
//! answers with a [`Response`]. Requests default to [`Action::Plan`], a dry run that only
//! evaluates [`Guard`]s. Nothing changes unless the caller asks to execute.

mod envelope;
mod error;
mod guard;
pub mod volume;

pub use envelope::{Action, Outcome, Request, Response};
pub use error::Error;
pub use guard::Guard;
