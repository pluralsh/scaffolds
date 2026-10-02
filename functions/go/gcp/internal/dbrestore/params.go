package dbrestore

import (
	"time"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/cloudsql"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// Params are the operation-specific fields of the tool input.
type Params struct {
	// Instance is the source instance.
	Instance string `json:"instance" required:"true"`
	// TargetInstance is the new instance to restore into.
	TargetInstance string `json:"targetInstance" required:"true"`
	// RestorePointInTime is the RFC 3339 time to restore to.
	RestorePointInTime string `json:"restorePointInTime" required:"true"`
}

// Validate checks the parameters before anything is asked of Cloud SQL, and returns the
// restore point.
func (p Params) Validate() (time.Time, error) {
	if !cloudsql.InstanceName(p.Instance).Valid() {
		return time.Time{}, core.InvalidRequestf("instance %q is not a Cloud SQL instance name", p.Instance)
	}
	if !cloudsql.InstanceName(p.TargetInstance).Valid() {
		return time.Time{}, core.InvalidRequestf("targetInstance %q is not a Cloud SQL instance name (lowercase letters, digits and hyphens, starting with a letter)", p.TargetInstance)
	}
	point, err := time.Parse(time.RFC3339, p.RestorePointInTime)
	if err != nil {
		return time.Time{}, core.InvalidRequestf("restorePointInTime %q is not an RFC 3339 timestamp", p.RestorePointInTime)
	}
	return point, nil
}

// Output is the result reported to the caller.
type Output struct {
	Source *Summary `json:"source,omitempty"`
	// EarliestRestorePoint and LatestRestorePoint bound the times the source can be restored
	// to, when known.
	EarliestRestorePoint string `json:"earliestRestorePoint,omitempty"`
	LatestRestorePoint   string `json:"latestRestorePoint,omitempty"`
	// Target is the instance under the target name, once it exists.
	Target *Summary `json:"target,omitempty"`
	// Submitted is whether the restore was submitted. Cloud SQL then creates the instance in
	// the background.
	Submitted bool `json:"submitted"`
	// Operation is the Cloud SQL operation of the restore, once submitted.
	Operation string `json:"operation,omitempty"`
}

// Summary describes an instance.
type Summary struct {
	Name            string `json:"name"`
	DatabaseVersion string `json:"databaseVersion,omitempty"`
	State           string `json:"state"`
	Region          string `json:"region,omitempty"`
	// ConnectionName is what the Cloud SQL connectors and Auth Proxy connect to,
	// project:region:name.
	ConnectionName string `json:"connectionName,omitempty"`
	// IPAddresses are the instance's addresses by type, e.g. PRIVATE or PRIMARY.
	IPAddresses map[string]string `json:"ipAddresses,omitempty"`
	CreateTime  string            `json:"createTime,omitempty"`
}
