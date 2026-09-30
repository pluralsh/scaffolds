package volume

import (
	"encoding/json"
	"os"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const (
	// SnapshotMaxAgeSecs is how old a snapshot may be. Snapshots started longer ago are
	// ignored, and a new one is taken instead.
	SnapshotMaxAgeSecs int64 = 24 * 60 * 60

	// ClockSkewSecs is the tolerated clock difference between the function and the cloud API.
	ClockSkewSecs int64 = 5 * 60

	// AllowSkipSnapshotVar is the environment variable that allows callers to skip the
	// snapshot with `snapshot: false`.
	AllowSkipSnapshotVar = "ALLOW_SKIP_SNAPSHOT"
)

// SnapshotState is the state of the pre-deletion snapshot.
type SnapshotState string

// The states a pre-deletion snapshot can be in.
const (
	SnapshotMissing    SnapshotState = "missing"
	SnapshotInProgress SnapshotState = "inProgress"
	SnapshotCompleted  SnapshotState = "completed"
	SnapshotFailed     SnapshotState = "failed"
)

// SnapshotStatus is the state of the most recent snapshot this function took of the volume.
// ID and Progress are unset for [SnapshotMissing], and Progress only applies to
// [SnapshotInProgress].
type SnapshotStatus struct {
	State    SnapshotState
	ID       string
	Progress *string
}

// MarshalJSON leaves out the fields that don't apply to the state.
func (s SnapshotStatus) MarshalJSON() ([]byte, error) {
	if s.State == SnapshotMissing {
		return json.Marshal(struct {
			State SnapshotState `json:"state"`
		}{s.State})
	}
	return json.Marshal(struct {
		State    SnapshotState `json:"state"`
		ID       string        `json:"id"`
		Progress *string       `json:"progress,omitempty"`
	}{s.State, s.ID, s.Progress})
}

// SnapshotWindow is the time in which a snapshot has to have been started to count, in Unix
// seconds.
type SnapshotWindow struct {
	Now int64
	// DetachedAt is when the volume was last detached, if the cloud reports it.
	DetachedAt *int64
}

// Counts reports whether a snapshot started at startedAt counts: it must be recent, not in
// the future and, when the detach time is known, taken after it.
func (w SnapshotWindow) Counts(startedAt int64) bool {
	age := w.Now - startedAt
	return age >= -ClockSkewSecs && age <= SnapshotMaxAgeSecs &&
		(w.DetachedAt == nil || startedAt > *w.DetachedAt)
}

// SnapshotPolicy is what the installation allows callers to do about the snapshot.
type SnapshotPolicy struct {
	// AllowSkip lets callers skip the snapshot with `snapshot: false`.
	AllowSkip bool
}

// SnapshotPolicyFromEnv reads the policy from [AllowSkipSnapshotVar].
func SnapshotPolicyFromEnv() SnapshotPolicy {
	return SnapshotPolicy{AllowSkip: os.Getenv(AllowSkipSnapshotVar) == "true"}
}

// Required reports whether a snapshot is required, given the snapshot parameter of the
// request, which defaults to true. Skipping the snapshot is rejected unless the policy
// allows it.
func (p SnapshotPolicy) Required(requested *bool) (bool, error) {
	if requested == nil || *requested {
		return true, nil
	}
	if !p.AllowSkip {
		return false, core.InvalidRequestf("snapshot: false is not allowed by this installation; the volume is always snapshotted first")
	}
	return false, nil
}
