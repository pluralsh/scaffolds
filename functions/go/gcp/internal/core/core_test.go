package core

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

type echoParams struct {
	VolumeID string  `json:"volumeId" required:"true"`
	Fail     *string `json:"fail"`
	Snapshot *bool   `json:"snapshot"`
}

func decodeEcho(t *testing.T, body string) (Action, echoParams, error) {
	t.Helper()
	req, err := ParseRequest[echoParams]([]byte(body))
	return req.Action, req.Params, err
}

func TestRequestDefaultsToPlan(t *testing.T) {
	action, params, err := decodeEcho(t, `{"volumeId": "vol-1"}`)

	if err != nil || action != ActionPlan || params.VolumeID != "vol-1" {
		t.Errorf("got %v, %+v, %v", action, params, err)
	}
}

func TestRequestReadsAction(t *testing.T) {
	action, _, err := decodeEcho(t, `{"action": "execute", "volumeId": "v"}`)

	if err != nil || action != ActionExecute {
		t.Errorf("got %v, %v", action, err)
	}
}

func TestRequestRejectsInvalidInput(t *testing.T) {
	tests := map[string]string{
		"unknown action":  `{"action":"delete","volumeId":"v"}`,
		"other action":    `{"action":"status","volumeId":"v"}`,
		"null action":     `{"action":null,"volumeId":"v"}`,
		"numeric action":  `{"action":1,"volumeId":"v"}`,
		"missing field":   `{"action":"plan"}`,
		"null field":      `{"volumeId":null}`,
		"wrong type":      `{"volumeId":1}`,
		"other spelling":  `{"volumeid":"v"}`,
		"not an object":   `[]`,
		"null":            `null`,
		"empty":           ``,
		"invalid json":    `{`,
		"trailing garble": `{"volumeId":"v"} x`,
	}
	for name, body := range tests {
		t.Run(name, func(t *testing.T) {
			_, _, err := decodeEcho(t, body)

			var e *Error
			if !errors.As(err, &e) || e.Kind != KindInvalidRequest {
				t.Errorf("err = %v, want an invalid request", err)
			}
		})
	}
}

func TestRequestMatchesFieldNamesExactly(t *testing.T) {
	_, params, err := decodeEcho(t, `{"volumeId":"v","SNAPSHOT":false,"unknown":1}`)

	if err != nil || params.Snapshot != nil {
		t.Errorf("got %+v, %v", params, err)
	}
}

func TestMissingFieldMessage(t *testing.T) {
	_, _, err := decodeEcho(t, `{}`)

	if want := "invalid request: missing field `volumeId`"; err == nil || err.Error() != want {
		t.Errorf("err = %v, want %q", err, want)
	}
}

func TestPlanIsRefusedWhenAnyGuardFails(t *testing.T) {
	guards := Guards{Pass("unattached", "no attachments"), Fail("tagged", "missing tag")}

	if got := Planned(guards, struct{}{}).Outcome; got != OutcomeRefused {
		t.Errorf("outcome = %v", got)
	}
	if got := Planned(guards[:1], struct{}{}).Outcome; got != OutcomePlanned {
		t.Errorf("outcome = %v", got)
	}
}

func TestResponseOmitsEmptyFields(t *testing.T) {
	body, err := canonicalJSON(Refused[struct{}](nil))

	if want := `{"action":"execute","outcome":"refused"}`; err != nil || string(body) != want {
		t.Errorf("got %s, %v", body, err)
	}
}

func TestCanonicalJSONSortsKeysAndKeepsMarkup(t *testing.T) {
	body, err := canonicalJSON(Done(Guards{Pass("a", "<b> & c")}, map[string]any{"z": 1, "a": int64(1) << 60}))

	want := `{"action":"execute","guards":[{"detail":"<b> & c","name":"a","passed":true}],"outcome":"done","result":{"a":1152921504606846976,"z":1}}`
	if err != nil || string(body) != want {
		t.Errorf("got %s, %v", body, err)
	}
}

// echo is a handler that plans with the volume ID it was given, or fails when asked to.
var echo = HandlerFunc[echoParams, map[string]any](func(_ context.Context, req Request[echoParams]) (Response[map[string]any], error) {
	if req.Params.Fail != nil {
		return Response[map[string]any]{}, Providerf("%s", *req.Params.Fail)
	}
	return Planned(Guards{Pass("ok", "ok")}, map[string]any{"volumeId": req.Params.VolumeID}), nil
})

func call(t *testing.T, method, body string) (int, map[string]any, string) {
	t.Helper()
	rec := httptest.NewRecorder()
	NewServer(echo).ServeHTTP(rec, httptest.NewRequest(method, "/", strings.NewReader(body)))

	var parsed map[string]any
	if rec.Body.Len() > 0 {
		if err := json.Unmarshal(rec.Body.Bytes(), &parsed); err != nil {
			t.Fatalf("body %q: %v", rec.Body.String(), err)
		}
	}
	return rec.Code, parsed, rec.Body.String()
}

func TestReturnsResponseEnvelope(t *testing.T) {
	status, body, _ := call(t, http.MethodPost, `{"volumeId":"vol-1"}`)

	if status != http.StatusOK || body["outcome"] != "planned" {
		t.Errorf("got %d, %v", status, body)
	}
	if result, _ := body["result"].(map[string]any); result["volumeId"] != "vol-1" {
		t.Errorf("result = %v", body["result"])
	}
}

func TestRejectsInvalidPayload(t *testing.T) {
	status, body, _ := call(t, http.MethodPost, `{"action":"delete","volumeId":"v"}`)

	if status != http.StatusBadRequest || body["errorType"] != "InvalidRequest" {
		t.Errorf("got %d, %v", status, body)
	}
}

func TestMapsProviderErrors(t *testing.T) {
	status, _, raw := call(t, http.MethodPost, `{"volumeId":"v","fail":"AccessDenied: nope"}`)

	want := `{"errorMessage":"cloud provider error: AccessDenied: nope","errorType":"Provider"}`
	if status != http.StatusBadGateway || raw != want {
		t.Errorf("got %d, %s", status, raw)
	}
}

func TestMapsOtherErrorsToProviderErrors(t *testing.T) {
	rec := httptest.NewRecorder()
	failing := HandlerFunc[echoParams, int](func(context.Context, Request[echoParams]) (Response[int], error) {
		return Response[int]{}, errors.New("boom")
	})
	NewServer(failing).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(`{"volumeId":"v"}`)))

	if rec.Code != http.StatusBadGateway || !strings.Contains(rec.Body.String(), "cloud provider error: boom") {
		t.Errorf("got %d, %s", rec.Code, rec.Body)
	}
}

func TestRejectsOtherMethods(t *testing.T) {
	for _, method := range []string{http.MethodGet, http.MethodPut, http.MethodDelete} {
		status, _, raw := call(t, method, "")

		if status != http.StatusMethodNotAllowed || raw != "" {
			t.Errorf("%s: got %d, %q", method, status, raw)
		}
	}
}

func TestEveryPathIsAnInvocation(t *testing.T) {
	rec := httptest.NewRecorder()
	NewServer(echo).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/some/path", strings.NewReader(`{"volumeId":"v"}`)))

	if rec.Code != http.StatusOK {
		t.Errorf("status = %d", rec.Code)
	}
}

func TestRejectsOversizedBodies(t *testing.T) {
	status, _, _ := call(t, http.MethodPost, `{"volumeId":"`+strings.Repeat("a", maxBodyBytes)+`"}`)

	if status != http.StatusRequestEntityTooLarge {
		t.Errorf("status = %d", status)
	}
}
