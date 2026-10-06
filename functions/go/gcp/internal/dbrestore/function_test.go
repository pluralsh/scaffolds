package dbrestore

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"slices"
	"strings"
	"testing"
	"time"

	sqladmin "google.golang.org/api/sqladmin/v1"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/cloudsql"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const (
	now      = "2026-10-02T08:00:00Z"
	point    = "2026-10-01T08:00:00Z"
	earliest = "2026-09-25T08:00:00Z"
	latest   = "2026-10-02T07:59:00Z"
)

// fakeSQL is Cloud SQL: it answers with the instances it holds and records the clones asked
// of it.
type fakeSQL struct {
	instances map[string]*cloudsql.Instance
	window    cloudsql.RecoveryWindow
	err       error

	clones []string
}

func (f *fakeSQL) Connect(context.Context) (cloudsql.Client, error) { return f, nil }

func (f *fakeSQL) Instance(_ context.Context, name string) (*cloudsql.Instance, error) {
	return f.instances[name], f.err
}

func (f *fakeSQL) RecoveryWindow(context.Context, string) (cloudsql.RecoveryWindow, error) {
	return f.window, nil
}

func (f *fakeSQL) Clone(_ context.Context, source, target, pointInTime string) (string, error) {
	f.clones = append(f.clones, source+">"+target+"@"+pointInTime)
	return "operation-clone", nil
}

// testSource is a running PostgreSQL primary with point-in-time recovery.
func testSource() *cloudsql.Instance {
	return &cloudsql.Instance{
		Name:            "db",
		DatabaseVersion: "POSTGRES_17",
		State:           "RUNNABLE",
		InstanceType:    "CLOUD_SQL_INSTANCE",
		Region:          "europe-central2",
		ConnectionName:  "p:europe-central2:db",
		IpAddresses:     []*sqladmin.IpMapping{{Type: "PRIVATE", IpAddress: "10.0.0.3"}},
		Settings: &sqladmin.Settings{BackupConfiguration: &sqladmin.BackupConfiguration{
			PointInTimeRecoveryEnabled: true, TransactionLogRetentionDays: 7,
		}},
	}
}

func newFake() *fakeSQL {
	return &fakeSQL{
		instances: map[string]*cloudsql.Instance{"db": testSource()},
		window:    cloudsql.RecoveryWindow{Earliest: earliest, Latest: latest},
	}
}

type fixedClock int64

func (c fixedClock) Now() int64 { return int64(c) }

func invoke(t *testing.T, fake *fakeSQL, body string) (int, string) {
	t.Helper()
	at, _ := time.Parse(time.RFC3339, now)
	rec := httptest.NewRecorder()
	fn := &Function{connector: fake, clock: fixedClock(at.Unix())}
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body)))
	return rec.Code, rec.Body.String()
}

func request(action, target, at string) string {
	body, _ := json.Marshal(map[string]string{"action": action, "instance": "db", "targetInstance": target, "restorePointInTime": at})
	return string(body)
}

func outcome(t *testing.T, body string) (string, []string) {
	t.Helper()
	var resp struct {
		Outcome string
		Guards  []core.Guard
	}
	if err := json.Unmarshal([]byte(body), &resp); err != nil {
		t.Fatalf("body %q: %v", body, err)
	}
	var failed []string
	for _, g := range resp.Guards {
		if !g.Passed {
			failed = append(failed, g.Name)
		}
	}
	return resp.Outcome, failed
}

func TestPlanDescribesTheRestore(t *testing.T) {
	fake := newFake()

	status, body := invoke(t, fake, request("plan", "db-restored", point))

	want := `{"action":"plan","guards":[` +
		`{"detail":"Instance db is RUNNABLE.","name":"source","passed":true},` +
		`{"detail":"Point-in-time recovery is enabled.","name":"point-in-time-recovery","passed":true},` +
		`{"detail":"Instance db-restored will be created as a copy of the source.","name":"new-instance","passed":true},` +
		`{"detail":"The restore point is between the earliest restore point (2026-09-25T08:00:00Z) and the latest (2026-10-02T07:59:00Z).","name":"restore-point","passed":true}],` +
		`"outcome":"planned","result":{"earliestRestorePoint":"2026-09-25T08:00:00Z","latestRestorePoint":"2026-10-02T07:59:00Z",` +
		`"source":{"connectionName":"p:europe-central2:db","databaseVersion":"POSTGRES_17","ipAddresses":{"PRIVATE":"10.0.0.3"},` +
		`"name":"db","region":"europe-central2","state":"RUNNABLE"},"submitted":false}}`
	if status != http.StatusOK || body != want {
		t.Errorf("got %d %s\nwant %s", status, body, want)
	}
	if len(fake.clones) != 0 {
		t.Errorf("cloned on plan: %v", fake.clones)
	}
}

func TestExecuteClones(t *testing.T) {
	fake := newFake()

	_, body := invoke(t, fake, request("execute", "db-restored", point))

	if got, failed := outcome(t, body); got != "done" || len(failed) != 0 {
		t.Errorf("got %s", body)
	}
	if !slices.Equal(fake.clones, []string{"db>db-restored@" + point}) ||
		!strings.Contains(body, `"operation":"operation-clone","source"`) || !strings.Contains(body, `"target":{"name":"db-restored","state":"PENDING_CREATE"}`) {
		t.Errorf("clones %v, body %s", fake.clones, body)
	}
}

func TestPlanReportsAnExistingTargetAndRefuses(t *testing.T) {
	fake := newFake()
	restored := testSource()
	restored.Name, restored.State, restored.CreateTime = "db-restored", "PENDING_CREATE", "2026-10-02T07:00:00Z"
	fake.instances["db-restored"] = restored

	_, body := invoke(t, fake, request("execute", "db-restored", point))

	if got, failed := outcome(t, body); got != "refused" || !slices.Equal(failed, []string{guardNewInstance}) || len(fake.clones) != 0 {
		t.Errorf("got %s, clones %v", body, fake.clones)
	}
	if !strings.Contains(body, `"target":{"connectionName":"p:europe-central2:db","createTime":"2026-10-02T07:00:00Z"`) {
		t.Errorf("target not reported: %s", body)
	}
}

func TestRefusesUnsafeRestoresWithoutChangingAnything(t *testing.T) {
	for name, tc := range map[string]struct {
		change func(*fakeSQL)
		target string
		point  string
		failed []string
	}{
		"source busy":       {func(f *fakeSQL) { f.instances["db"].State = "MAINTENANCE" }, "db-restored", point, []string{guardSource}},
		"replica":           {func(f *fakeSQL) { f.instances["db"].InstanceType = "READ_REPLICA_INSTANCE" }, "db-restored", point, []string{guardSource}},
		"no recovery":       {func(f *fakeSQL) { f.instances["db"].Settings.BackupConfiguration.PointInTimeRecoveryEnabled = false }, "db-restored", point, []string{guardRecovery}},
		"over the source":   {func(*fakeSQL) {}, "db", point, []string{guardNewInstance}},
		"before the window": {func(*fakeSQL) {}, "db-restored", "2026-09-20T08:00:00Z", []string{guardRestorePoint}},
		"after the window":  {func(*fakeSQL) {}, "db-restored", now, []string{guardRestorePoint}},
		"missing source":    {func(f *fakeSQL) { delete(f.instances, "db") }, "db-restored", point, []string{guardSource}},
	} {
		t.Run(name, func(t *testing.T) {
			fake := newFake()
			tc.change(fake)

			_, body := invoke(t, fake, request("execute", tc.target, tc.point))

			if got, failed := outcome(t, body); got != "refused" || !slices.Equal(failed, tc.failed) || len(fake.clones) != 0 {
				t.Errorf("got %s, clones %v, want refused with %v failed", body, fake.clones, tc.failed)
			}
		})
	}
}

func TestMySQLBinaryLoggingAllowsRestores(t *testing.T) {
	fake := newFake()
	backups := fake.instances["db"].Settings.BackupConfiguration
	backups.PointInTimeRecoveryEnabled, backups.BinaryLogEnabled = false, true

	_, body := invoke(t, fake, request("plan", "db-restored", point))

	if got, _ := outcome(t, body); got != "planned" {
		t.Errorf("got %s", body)
	}
}

func TestEstimatesTheEarliestPointFromTheLogRetention(t *testing.T) {
	fake := newFake()
	fake.window = cloudsql.RecoveryWindow{}

	_, body := invoke(t, fake, request("plan", "db-restored", point))

	if got, _ := outcome(t, body); got != "planned" || !strings.Contains(body, "(2026-09-25T08:00:00Z) and the latest (2026-10-02T08:00:00Z)") {
		t.Errorf("got %s", body)
	}
}

func TestProviderErrorsAreReported(t *testing.T) {
	fake := newFake()
	fake.err = core.Providerf("Cloud SQL Admin API: boom")

	status, body := invoke(t, fake, request("plan", "db-restored", point))

	if status != http.StatusBadGateway || body != `{"errorMessage":"cloud provider error: Cloud SQL Admin API: boom","errorType":"Provider"}` {
		t.Errorf("got %d %s", status, body)
	}
}

func TestInvalidParamsAreRejected(t *testing.T) {
	for name, body := range map[string]string{
		"instance":           `{"instance":"DB","targetInstance":"db-restored","restorePointInTime":"` + point + `"}`,
		"targetInstance":     `{"instance":"db","targetInstance":"db_restored","restorePointInTime":"` + point + `"}`,
		"restorePointInTime": `{"instance":"db","targetInstance":"db-restored","restorePointInTime":"yesterday"}`,
	} {
		status, got := invoke(t, nil, body)

		if status != http.StatusBadRequest || !strings.Contains(got, name+" ") {
			t.Errorf("%s: got %d %s", name, status, got)
		}
	}
}
