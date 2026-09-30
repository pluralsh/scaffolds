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
}

/// Tool input sent by the workbench. Operation-specific fields sit next to the envelope
/// fields, e.g. `{"action": "execute", "volumeId": "vol-123"}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request<P> {
    #[serde(default)]
    pub action: Action,
    #[serde(flatten)]
    pub params: P,
}

/// Final state of a single invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Plan finished and every guard passed.
    Planned,
    /// At least one guard failed and the operation was not performed. A function may still
    /// have started a preparatory step, such as a snapshot, which it then reports in the result.
    Refused,
    /// The change was submitted. Functions return without waiting for it to complete and
    /// report the resource state they observed instead.
    Done,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response<R> {
    pub action: Action,
    pub outcome: Outcome,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub guards: Vec<Guard>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<R>,
}

impl<R> Response<R> {
    /// Response to a plan: [`Outcome::Planned`] if every guard passed, [`Outcome::Refused`] otherwise.
    pub fn planned(guards: Vec<Guard>, result: R) -> Self {
        let outcome = if guards.iter().all(|g| g.passed) {
            Outcome::Planned
        } else {
            Outcome::Refused
        };
        Self {
            action: Action::Plan,
            outcome,
            guards,
            result: Some(result),
        }
    }

    /// Execute refused because at least one guard failed.
    pub fn refused(guards: Vec<Guard>) -> Self {
        Self {
            action: Action::Execute,
            outcome: Outcome::Refused,
            guards,
            result: None,
        }
    }

    pub fn done(guards: Vec<Guard>, result: R) -> Self {
        Self {
            action: Action::Execute,
            outcome: Outcome::Done,
            guards,
            result: Some(result),
        }
    }

    /// Attaches a result, e.g. to explain a refused execute.
    pub fn with_result(mut self, result: R) -> Self {
        self.result = Some(result);
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value, json};

    use super::*;

    #[test]
    fn request_defaults_to_plan() {
        let req: Request<Map<String, Value>> =
            serde_json::from_value(json!({"volumeId": "vol-1"})).unwrap();

        assert_eq!(req.action, Action::Plan);
        assert_eq!(req.params["volumeId"], "vol-1");
    }

    #[test]
    fn request_reads_action() {
        let req: Request<Map<String, Value>> =
            serde_json::from_value(json!({"action": "execute"})).unwrap();

        assert_eq!(req.action, Action::Execute);
        assert!(req.params.is_empty());
    }

    #[test]
    fn request_rejects_unknown_action() {
        for action in ["delete", "status"] {
            let res =
                serde_json::from_value::<Request<Map<String, Value>>>(json!({"action": action}));

            assert!(res.is_err(), "{action} should be rejected");
        }
    }

    #[test]
    fn plan_is_refused_when_any_guard_fails() {
        let guards = vec![
            Guard::pass("unattached", "no attachments"),
            Guard::fail("tagged", "missing tag"),
        ];

        assert_eq!(Response::planned(guards, ()).outcome, Outcome::Refused);
    }

    #[test]
    fn response_omits_empty_fields() {
        let body = serde_json::to_value(Response::<()>::refused(vec![])).unwrap();

        assert_eq!(body, json!({"action": "execute", "outcome": "refused"}));
    }
}
