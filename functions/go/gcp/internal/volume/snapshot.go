package volume

import (
	"encoding/json"
	"os"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const (
	// SnapshotMaxAgeSecs is the maximum age of a snapshot that counts. Older ones are ignored
	// and a new one is taken.
	SnapshotMaxAgeSecs int64 = 24 * 60 * 60

	// ClockSkewSecs is the tolerated clock difference between the function and the cloud API.
	ClockSkewSecs int64 = 5 * 60

	// AllowSkipSnapshotVar lets callers skip the snapshot with `snapshot: false` when set to
	// "true".
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

// SnapshotStatus is the state of the latest snapshot this function took of the volume. ID and
// Progress are unset for [SnapshotMissing]. Progress only applies to [SnapshotInProgress].
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

// SnapshotWindow is the time range, in Unix seconds, in which a snapshot must have started
// to count.
type SnapshotWindow struct {
	Now int64
	// DetachedAt is when the volume was last detached, if the cloud reports it.
	DetachedAt *int64
}

// Counts reports whether a snapshot started at startedAt counts. It must be recent, not in
// the future (beyond [ClockSkewSecs]), and after the last detach if that is known.
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

// Required reports whether a snapshot is required. requested is the request's snapshot
// parameter, which defaults to true. Skipping is rejected unless the policy allows it.
func (p SnapshotPolicy) Required(requested *bool) (bool, error) {
	if requested == nil || *requested {
		return true, nil
	}
	if !p.AllowSkip {
		return false, core.InvalidRequestf("snapshot: false is not allowed by this installation; the volume is always snapshotted first")
	}
	return false, nil
}
