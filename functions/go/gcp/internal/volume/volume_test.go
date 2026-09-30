package volume

import (
	"encoding/json"
	"errors"
	"slices"
	"strings"
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

func orphan() *Volume {
	return &Volume{
		ID:         "vol-1",
		State:      "available",
		Deletable:  true,
		SizeGiB:    new(int64(10)),
		Kubernetes: ClaimFromMetadata("data", "apps", "pv-1"),
	}
}

func completed() SnapshotStatus {
	return SnapshotStatus{State: SnapshotCompleted, ID: "snap-1"}
}

func missing() SnapshotStatus {
	return SnapshotStatus{State: SnapshotMissing}
}

// evaluate decides the deletion of vol, created for the PersistentVolume pv.
func evaluate(action core.Action, vol *Volume, pv PVName, snapshotRequired bool, snapshot SnapshotStatus) (core.Guards, Step) {
	return Deletion{Action: action, Volume: vol, PV: pv, SnapshotRequired: snapshotRequired, Snapshot: snapshot}.Evaluate()
}

func failedNames(guards core.Guards) []string {
	var names []string
	for _, g := range guards {
		if !g.Passed {
			names = append(names, g.Name)
		}
	}
	return names
}

func assertFailed(t *testing.T, guards core.Guards, want ...string) {
	t.Helper()
	if got := failedNames(guards); !slices.Equal(got, want) {
		t.Errorf("failed guards = %v, want %v", got, want)
	}
}

func assertStep(t *testing.T, got, want Step) {
	t.Helper()
	if got != want {
		t.Errorf("step = %v, want %v", got, want)
	}
}

func TestDeletesOrphanAfterCompletedSnapshot(t *testing.T) {
	guards, step := evaluate(core.ActionExecute, orphan(), "pv-1", true, completed())

	assertFailed(t, guards)
	assertStep(t, step, StepDelete)
}

func TestDeletesOrphanWithoutSnapshotWhenNotRequested(t *testing.T) {
	guards, step := evaluate(core.ActionExecute, orphan(), "pv-1", false, missing())

	assertFailed(t, guards)
	assertStep(t, step, StepDelete)
}

func TestPlanNeverChangesAnything(t *testing.T) {
	for _, snapshot := range []SnapshotStatus{missing(), completed()} {
		guards, step := evaluate(core.ActionPlan, orphan(), "pv-1", true, snapshot)

		assertFailed(t, guards)
		assertStep(t, step, StepNothing)
	}
}

func TestExecuteTakesSnapshotFirst(t *testing.T) {
	for _, snapshot := range []SnapshotStatus{missing(), {State: SnapshotFailed, ID: "snap-0"}} {
		guards, step := evaluate(core.ActionExecute, orphan(), "pv-1", true, snapshot)

		assertFailed(t, guards, "snapshot")
		assertStep(t, step, StepCreateSnapshot)
	}
}

func TestWaitsForSnapshotInProgress(t *testing.T) {
	snapshot := SnapshotStatus{State: SnapshotInProgress, ID: "snap-1", Progress: new("40%")}
	guards, step := evaluate(core.ActionExecute, orphan(), "pv-1", true, snapshot)

	assertFailed(t, guards, "snapshot")
	if detail := guards[len(guards)-1].Detail; !strings.Contains(detail, "40%") {
		t.Errorf("detail %q does not mention the progress", detail)
	}
	assertStep(t, step, StepNothing)
}

func TestRefusesMissingVolume(t *testing.T) {
	guards, step := evaluate(core.ActionExecute, nil, "pv-1", false, missing())

	assertFailed(t, guards, "exists")
	assertStep(t, step, StepNothing)
}

func TestRefusesAttachedVolumeWithoutSnapshotting(t *testing.T) {
	vol := orphan()
	vol.AttachedTo = []string{"i-1"}
	guards, step := evaluate(core.ActionExecute, vol, "pv-1", true, missing())

	if !slices.Contains(failedNames(guards), "unattached") {
		t.Errorf("failed guards = %v, want unattached among them", failedNames(guards))
	}
	assertStep(t, step, StepNothing)
}

func TestRefusesVolumeInNonDeletableState(t *testing.T) {
	vol := orphan()
	vol.State, vol.Deletable = "creating", false
	guards, step := evaluate(core.ActionExecute, vol, "pv-1", false, completed())

	assertFailed(t, guards, "state")
	assertStep(t, step, StepNothing)
}

func TestRefusesVolumeNotCreatedByKubernetes(t *testing.T) {
	vol := orphan()
	vol.Kubernetes = nil
	guards, step := evaluate(core.ActionExecute, vol, "pv-1", false, completed())

	assertFailed(t, guards, "kubernetes")
	assertStep(t, step, StepNothing)
}

func TestOnlyRecentSnapshotsAfterTheLastDetachCount(t *testing.T) {
	const now = 1_000_000
	tests := []struct {
		name       string
		startedAt  int64
		detachedAt *int64
		want       bool
	}{
		{"now", now, nil, true},
		{"oldest allowed", now - SnapshotMaxAgeSecs, nil, true},
		{"too old", now - SnapshotMaxAgeSecs - 1, nil, false},
		{"future within clock skew", now + ClockSkewSecs, nil, true},
		{"future beyond clock skew", now + ClockSkewSecs + 1, nil, false},
		{"after detach", now - 10, new(int64(now - 20)), true},
		{"before detach", now - 20, new(int64(now - 10)), false},
		{"at detach", now - 20, new(int64(now - 20)), false},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			window := SnapshotWindow{Now: now, DetachedAt: tt.detachedAt}
			if got := window.Counts(tt.startedAt); got != tt.want {
				t.Errorf("Counts() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestRefusesVolumeOfAnotherPersistentVolume(t *testing.T) {
	guards, step := evaluate(core.ActionExecute, orphan(), "pv-2", false, completed())

	assertFailed(t, guards, "kubernetes")
	if detail := guards[3].Detail; !strings.Contains(detail, "pv-1, not pv-2") {
		t.Errorf("detail = %q", detail)
	}
	assertStep(t, step, StepNothing)
}

func TestRefusesClaimWithoutPersistentVolumeName(t *testing.T) {
	vol := orphan()
	vol.Kubernetes = ClaimFromMetadata("data", "apps", "")
	guards, step := evaluate(core.ActionExecute, vol, "pv-1", false, completed())

	assertFailed(t, guards, "kubernetes")
	assertStep(t, step, StepNothing)
}

func TestValidatesPersistentVolumeNames(t *testing.T) {
	for _, pv := range []PVName{"pvc-0a1b2c3d-1111-2222-3333-444455556666", "pv-1", "a.b"} {
		if err := pv.Validate(); err != nil {
			t.Errorf("%q: %v", pv, err)
		}
	}
	for _, pv := range []PVName{"", "PV-1", "-pv", "pv-", "pv_1", "a/b", PVName(strings.Repeat("a", 254))} {
		if err := pv.Validate(); err == nil {
			t.Errorf("%q should be rejected", pv)
		}
	}
}

func TestSkippingTheSnapshotNeedsTheInstallationToAllowIt(t *testing.T) {
	tests := []struct {
		requested       *bool
		allowSkip       bool
		want, wantError bool
	}{
		{nil, false, true, false},
		{new(true), false, true, false},
		{new(true), true, true, false},
		{new(false), true, false, false},
		{new(false), false, false, true},
	}
	for _, tt := range tests {
		got, err := SnapshotPolicy{AllowSkip: tt.allowSkip}.Required(tt.requested)
		if got != tt.want || (err != nil) != tt.wantError {
			t.Errorf("Required(%v) with allowSkip %v = %v, %v", tt.requested, tt.allowSkip, got, err)
		}
		var e *core.Error
		if err != nil && (!errors.As(err, &e) || e.Kind != core.KindInvalidRequest) {
			t.Errorf("error = %v, want an invalid request", err)
		}
	}
}

func TestSnapshotPolicyReadsTheEnvironment(t *testing.T) {
	for value, want := range map[string]bool{"": false, "true": true, "yes": false} {
		t.Setenv(AllowSkipSnapshotVar, value)
		if got := SnapshotPolicyFromEnv().AllowSkip; got != want {
			t.Errorf("%q: AllowSkip = %v, want %v", value, got, want)
		}
	}
}

func TestParsesCloudTimestamps(t *testing.T) {
	tests := []struct {
		value  string
		want   int64
		wantOK bool
	}{
		{"1970-01-01T00:01:00Z", 60, true},
		{"2026-09-30T10:00:00.123-07:00", 1_790_787_600, true},
		{"2026-09-30T17:00:00.1234567+00:00", 1_790_787_600, true},
		{"yesterday", 0, false},
	}
	for _, tt := range tests {
		got, ok := ParseTimestamp(tt.value)
		if got != tt.want || ok != tt.wantOK {
			t.Errorf("ParseTimestamp(%q) = %d, %v", tt.value, got, ok)
		}
	}
}

func TestClaimRequiresPVCName(t *testing.T) {
	if ClaimFromMetadata("", "apps", "pv") != nil || ClaimFromMetadata(" ", "", "") != nil {
		t.Error("a claim needs a PVC name")
	}
	got := ClaimFromMetadata("data", "", "")
	if got == nil || *got != (KubernetesClaim{PVC: "data"}) {
		t.Errorf("claim = %+v", got)
	}
}

func TestSerializesLikeTheRustFunctions(t *testing.T) {
	vol := orphan()
	vol.AttachedTo = []string{"node-1"}
	vol.DetachedAt = new(int64(60))
	tests := []struct {
		name  string
		value any
		want  string
	}{
		{"volume", vol, `{"id":"vol-1","state":"available","deletable":true,"attachedTo":["node-1"],"sizeGib":10,"kubernetes":{"pvc":"data","namespace":"apps","pv":"pv-1"},"detachedAt":60}`},
		{"bare volume", Volume{ID: "d", State: "READY"}, `{"id":"d","state":"READY","deletable":false}`},
		{"missing", missing(), `{"state":"missing"}`},
		{"in progress", SnapshotStatus{State: SnapshotInProgress, ID: "s", Progress: new("uploading")}, `{"state":"inProgress","id":"s","progress":"uploading"}`},
		{"in progress without progress", SnapshotStatus{State: SnapshotInProgress, ID: "s"}, `{"state":"inProgress","id":"s"}`},
		{"completed", completed(), `{"state":"completed","id":"snap-1"}`},
		{"failed", SnapshotStatus{State: SnapshotFailed, ID: "s"}, `{"state":"failed","id":"s"}`},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := json.Marshal(tt.value)
			if err != nil || string(got) != tt.want {
				t.Errorf("got %s, %v; want %s", got, err, tt.want)
			}
		})
	}
}
