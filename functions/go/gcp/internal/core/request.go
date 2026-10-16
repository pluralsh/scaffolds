package core

import (
	"encoding/json"
	"reflect"
	"strings"
)

// Request is the tool input sent by the workbench. Operation-specific fields sit next to
// "action", e.g. {"action": "execute", "volumeId": "vol-123"}, and are decoded into Params.
type Request[P any] struct {
	Action Action
	Params P
}

// ParseRequest reads the tool input. The action defaults to [ActionPlan].
//
// Fields are matched by their exact JSON name, and unknown fields are ignored. A field of P
// tagged `required:"true"` must be present and not null.
func ParseRequest[P any](body []byte) (Request[P], error) {
	var req Request[P]
	var input toolInput
	if err := json.Unmarshal(body, &input); err != nil {
		return req, InvalidRequestf("%v", err)
	}
	if input == nil {
		return req, InvalidRequestf("invalid type: null, expected struct Request")
	}

	action, err := input.action()
	if err != nil {
		return req, err
	}
	if err := input.params(&req.Params); err != nil {
		return req, err
	}
	req.Action = action
	return req, nil
}

// toolInput is the tool input with its fields not yet decoded.
type toolInput map[string]json.RawMessage

const (
	actionField = "action"
	jsonNull    = "null"
)

// action returns the requested action, or [ActionPlan] if there is none.
func (in toolInput) action() (Action, error) {
	raw, ok := in[actionField]
	if !ok {
		return ActionPlan, nil
	}
	if string(raw) == jsonNull {
		return "", InvalidRequestf("invalid type: null, expected `plan` or `execute`")
	}
	var action Action
	if err := json.Unmarshal(raw, &action); err != nil {
		return "", InvalidRequestf("%v", err)
	}
	return action, nil
}

// params decodes the operation-specific fields into the struct that into points to.
func (in toolInput) params(into any) error {
	own := make(toolInput)
	for field := range reflect.TypeOf(into).Elem().Fields() {
		name, _, _ := strings.Cut(field.Tag.Get("json"), ",")
		if !field.IsExported() || name == "-" {
			continue
		}
		if name == "" {
			name = field.Name
		}
		raw, ok := in[name]
		if field.Tag.Get("required") == "true" && (!ok || string(raw) == jsonNull) {
			return InvalidRequestf("missing field `%s`", name)
		}
		if ok {
			own[name] = raw
		}
	}
	// encoding/json matches names case-insensitively. Re-encoding only the exact names
	// makes the match exact.
	filtered, err := json.Marshal(own)
	if err != nil {
		return InvalidRequestf("%v", err)
	}
	if err := json.Unmarshal(filtered, into); err != nil {
		return InvalidRequestf("%v", err)
	}
	return nil
}
