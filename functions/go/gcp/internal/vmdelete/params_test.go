package vmdelete

import (
	"strings"
	"testing"
)

func TestValidatesParams(t *testing.T) {
	if err := (Params{Zone: "us-central1-a", Instance: "vm-1"}).Validate(); err != nil {
		t.Errorf("valid params rejected: %v", err)
	}
	for name, params := range map[string]Params{
		"zone":     {Zone: "us-central1", Instance: "vm-1"},
		"instance": {Zone: "us-central1-a", Instance: "Vm_1"},
	} {
		if err := params.Validate(); err == nil || !strings.Contains(err.Error(), name) {
			t.Errorf("invalid %s: err = %v", name, err)
		}
	}
}
