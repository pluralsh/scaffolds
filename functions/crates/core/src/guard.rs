use serde::Serialize;

/// Result of a single safety check evaluated before an action is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Guard {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

impl Guard {
    pub fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), passed: true, detail: detail.into() }
    }

    pub fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { name: name.into(), passed: false, detail: detail.into() }
    }

    pub fn check(name: impl Into<String>, passed: bool, detail: impl Into<String>) -> Self {
        Self { name: name.into(), passed, detail: detail.into() }
    }
}
