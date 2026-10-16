package core

import (
	"encoding/json"
	"fmt"
)

// Action is what the caller wants the function to do.
type Action string

const (
	// ActionPlan evaluates guards and describes the change without making it.
	ActionPlan Action = "plan"
	// ActionExecute re-evaluates guards and makes the change if they all pass.
	ActionExecute Action = "execute"
)

// UnmarshalJSON accepts only the known actions.
func (a *Action) UnmarshalJSON(data []byte) error {
	var s string
	if err := json.Unmarshal(data, &s); err != nil {
		return err
	}
	switch Action(s) {
	case ActionPlan, ActionExecute:
		*a = Action(s)
		return nil
	}
	return fmt.Errorf("unknown variant `%s`, expected `plan` or `execute`", s)
}
