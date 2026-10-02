package dbrestore

import (
	"fmt"
	"time"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/cloudsql"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// The names of the guards, in the order they are evaluated.
const (
	guardSource       = "source"
	guardRecovery     = "point-in-time-recovery"
	guardNewInstance  = "new-instance"
	guardRestorePoint = "restore-point"
)

const (
	// stateRunnable is the state of an instance that is up and not being changed.
	stateRunnable = "RUNNABLE"
	// typePrimary is the type of an instance that isn't a replica.
	typePrimary = "CLOUD_SQL_INSTANCE"
)

// source is the Cloud SQL instance to restore.
type source struct {
	*cloudsql.Instance
}

func (s source) guard() core.Guard {
	if s.InstanceType != "" && s.InstanceType != typePrimary {
		return core.Fail(guardSource, fmt.Sprintf("instance %s is a %s; restore its primary instead", s.Name, s.InstanceType))
	}
	if s.State != stateRunnable {
		return core.Fail(guardSource, fmt.Sprintf("instance %s is %s, not %s", s.Name, s.State, stateRunnable))
	}
	return core.Pass(guardSource, fmt.Sprintf("instance %s is %s", s.Name, s.State))
}

// recoverable reports whether the source keeps the logs a point-in-time restore needs: with
// point-in-time recovery enabled, or, for MySQL, binary logging.
func (s source) recoverable() bool {
	if s.Settings == nil || s.Settings.BackupConfiguration == nil {
		return false
	}
	backups := s.Settings.BackupConfiguration
	return backups.PointInTimeRecoveryEnabled || backups.BinaryLogEnabled
}

func (s source) recoveryGuard() core.Guard {
	if s.recoverable() {
		return core.Pass(guardRecovery, "point-in-time recovery is enabled")
	}
	return core.Fail(guardRecovery, "point-in-time recovery is disabled; the instance can't be restored to a point in time")
}

// logRetention is how long the source keeps the logs a restore replays, or zero if unknown.
func (s source) logRetention() time.Duration {
	if s.Settings == nil || s.Settings.BackupConfiguration == nil {
		return 0
	}
	return time.Duration(s.Settings.BackupConfiguration.TransactionLogRetentionDays) * 24 * time.Hour
}

// window is the time range a restore can go back to.
type window struct {
	earliest, latest time.Time
}

// newWindow is the range Cloud SQL reported, with what it left out estimated: the earliest
// point from the log retention, the latest from now.
func newWindow(reported cloudsql.RecoveryWindow, retention time.Duration, now time.Time) window {
	w := window{latest: now}
	if latest, err := time.Parse(time.RFC3339, reported.Latest); err == nil {
		w.latest = latest
	}
	if earliest, err := time.Parse(time.RFC3339, reported.Earliest); err == nil {
		w.earliest = earliest
	} else if retention > 0 {
		w.earliest = now.Add(-retention)
	}
	return w
}

func (w window) guard(point time.Time) core.Guard {
	if w.earliest.IsZero() {
		return core.Fail(guardRestorePoint, "the earliest restore point is unknown")
	}
	return core.Check(guardRestorePoint, !point.Before(w.earliest) && !point.After(w.latest), fmt.Sprintf(
		"must be between the earliest restore point (%s) and the latest (%s)", w.earliest.UTC().Format(time.RFC3339), w.latest.UTC().Format(time.RFC3339)))
}

// targetGuard checks that the restore creates a new instance.
func targetGuard(params Params, existing *cloudsql.Instance) core.Guard {
	switch {
	case params.TargetInstance == params.Instance:
		return core.Fail(guardNewInstance, "the target must be a new instance; restoring over the source isn't supported")
	case existing != nil:
		return core.Fail(guardNewInstance, fmt.Sprintf(
			"an instance named %s already exists (%s, created %s) and is left alone; if an earlier execute restored into it, it is the restore, otherwise pick another name",
			existing.Name, existing.State, existing.CreateTime))
	}
	return core.Pass(guardNewInstance, params.TargetInstance+" will be created as a copy of the source")
}

func summary(instance *cloudsql.Instance) *Summary {
	if instance == nil {
		return nil
	}
	s := &Summary{
		Name:            instance.Name,
		DatabaseVersion: instance.DatabaseVersion,
		State:           instance.State,
		Region:          instance.Region,
		ConnectionName:  instance.ConnectionName,
		CreateTime:      instance.CreateTime,
	}
	for _, ip := range instance.IpAddresses {
		if s.IPAddresses == nil {
			s.IPAddresses = map[string]string{}
		}
		s.IPAddresses[ip.Type] = ip.IpAddress
	}
	return s
}
