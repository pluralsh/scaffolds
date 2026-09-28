use serde::{Deserialize, Serialize};

use crate::Guard;

/// What the caller wants the function to do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Evaluate guards and describe the change without making it.
    #[default]
    Plan,
    /// Re-evaluate guards and make the change if they all pass.
    Execute,
    /// Report progress of a long-running operation started by a previous execute.
    Status,
}

/// Tool input sent by the workbench. Operation-specific fields sit next to the envelope
/// fields, e.g. `{"action": "execute", "volumeId": "vol-123"}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request<P> {
    #[serde(default)]
    pub action: Action,
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(flatten)]
    pub params: P,
}

/// Final state of a single invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Plan finished and every guard passed.
    Planned,
    /// At least one guard failed, nothing was changed.
    Refused,
    /// The change was made.
    Done,
    /// The change was started and can be followed with [`Action::Status`].
    Pending,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response<R> {
    pub action: Action,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<Guard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
}

impl<R> Response<R> {
    /// Response to a plan: [`Outcome::Planned`] if every guard passed, [`Outcome::Refused`] otherwise.
    pub fn planned(guards: Vec<Guard>, result: R) -> Self {
        let outcome = if guards.iter().all(|g| g.passed) { Outcome::Planned } else { Outcome::Refused };
        Self { action: Action::Plan, outcome, guards, operation_id: None, result: Some(result) }
    }

    /// Execute refused because at least one guard failed.
    pub fn refused(guards: Vec<Guard>) -> Self {
        Self { action: Action::Execute, outcome: Outcome::Refused, guards, operation_id: None, result: None }
    }

    pub fn done(guards: Vec<Guard>, result: R) -> Self {
        Self { action: Action::Execute, outcome: Outcome::Done, guards, operation_id: None, result: Some(result) }
    }

    pub fn pending(guards: Vec<Guard>, operation_id: impl Into<String>) -> Self {
        Self {
            action: Action::Execute,
            outcome: Outcome::Pending,
            guards,
            operation_id: Some(operation_id.into()),
            result: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::*;

    #[test]
    fn request_defaults_to_plan() {
        let req: Request<Map<String, Value>> = serde_json::from_value(json!({"volumeId": "vol-1"})).unwrap();

        assert_eq!(req.action, Action::Plan);
        assert_eq!(req.operation_id, None);
        assert_eq!(req.params["volumeId"], "vol-1");
    }

    #[test]
    fn request_reads_envelope_fields() {
        let req: Request<Map<String, Value>> =
            serde_json::from_value(json!({"action": "status", "operationId": "op-1"})).unwrap();

        assert_eq!(req.action, Action::Status);
        assert_eq!(req.operation_id.as_deref(), Some("op-1"));
        assert!(req.params.is_empty());
    }

    #[test]
    fn request_rejects_unknown_action() {
        let res = serde_json::from_value::<Request<Map<String, Value>>>(json!({"action": "delete"}));

        assert!(res.is_err());
    }

    #[test]
    fn plan_is_refused_when_any_guard_fails() {
        let guards = vec![Guard::pass("unattached", "no attachments"), Guard::fail("tagged", "missing tag")];

        assert_eq!(Response::planned(guards, ()).outcome, Outcome::Refused);
    }

    #[test]
    fn response_omits_empty_fields() {
        let body = serde_json::to_value(Response::<()>::refused(vec![])).unwrap();

        assert_eq!(body, json!({"action": "execute", "outcome": "refused"}));
    }
}
