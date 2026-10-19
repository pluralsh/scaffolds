package core

// Outcome is the final state of a single invocation.
type Outcome string

const (
	// OutcomePlanned means the plan finished and every guard passed.
	OutcomePlanned Outcome = "planned"
	// OutcomeRefused means a guard failed and the operation was not performed. The function
	// may still have started a preparatory step, such as a snapshot, and reports it in the
	// result.
	OutcomeRefused Outcome = "refused"
	// OutcomeDone means the change was submitted. Functions don't wait for it to complete;
	// they report the resource state they saw.
	OutcomeDone Outcome = "done"
)

// Response is the body of a successful invocation.
type Response[R any] struct {
	Action  Action  `json:"action"`
	Outcome Outcome `json:"outcome"`
	Guards  Guards  `json:"guards,omitempty"`
	Result  *R      `json:"result,omitempty"`
}

// Planned is the response to a plan: [OutcomePlanned] if every guard passed,
// [OutcomeRefused] otherwise.
func Planned[R any](guards Guards, result R) Response[R] {
	outcome := OutcomePlanned
	if !guards.Passed() {
		outcome = OutcomeRefused
	}
	return Response[R]{Action: ActionPlan, Outcome: outcome, Guards: guards, Result: &result}
}

// Refused is the response to an execute refused because a guard failed.
func Refused[R any](guards Guards) Response[R] {
	return Response[R]{Action: ActionExecute, Outcome: OutcomeRefused, Guards: guards}
}

// Done is the response to an execute that submitted the change.
func Done[R any](guards Guards, result R) Response[R] {
	return Response[R]{Action: ActionExecute, Outcome: OutcomeDone, Guards: guards, Result: &result}
}

// WithResult attaches a result, e.g. to explain a refused execute.
func (r Response[R]) WithResult(result R) Response[R] {
	r.Result = &result
	return r
}
