package core

import (
	"bytes"
	"encoding/json"
	"testing"
)

func TestLoggerWritesCloudLoggingFields(t *testing.T) {
	var buf bytes.Buffer
	logger := NewLogger(&buf)

	logger.Warn("careful", "zone", "us-central1-a")
	logger.Error("boom")

	dec := json.NewDecoder(&buf)
	var warn, failure map[string]any
	if err := dec.Decode(&warn); err != nil {
		t.Fatal(err)
	}
	if err := dec.Decode(&failure); err != nil {
		t.Fatal(err)
	}
	if warn["severity"] != "WARNING" || warn["message"] != "careful" || warn["zone"] != "us-central1-a" {
		t.Errorf("warn = %v", warn)
	}
	if failure["severity"] != "ERROR" || failure["message"] != "boom" {
		t.Errorf("failure = %v", failure)
	}
	if _, ok := warn["level"]; ok {
		t.Errorf("level should be renamed: %v", warn)
	}
}
