package volumedelete

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volume"
)

// fakeCompute is a Compute Engine that holds one disk and records what is asked of it.
type fakeCompute struct {
	// Instances, groups, access and load balancers aren't used by this function; calling them panics.
	compute.Instances
	compute.Groups
	compute.Access
	compute.LoadBalancers

	disk      *compute.Disk
	snapshots []*compute.Snapshot
	err       error

	listed  []string
	created []compute.SnapshotRequest
	deleted []string
}

func (f *fakeCompute) Connect(context.Context) (compute.Client, error) { return f, nil }

func (f *fakeCompute) Disk(_ context.Context, _, _ string) (*compute.Disk, error) {
	return f.disk, f.err
}

func (f *fakeCompute) Snapshots(_ context.Context, label, value string) ([]*compute.Snapshot, error) {
	f.listed = append(f.listed, label+"="+value)
	return f.snapshots, f.err
}

func (f *fakeCompute) CreateSnapshot(_ context.Context, req compute.SnapshotRequest) (string, error) {
	f.created = append(f.created, req)
	return "operation-snapshot", f.err
}

func (f *fakeCompute) DeleteDisk(_ context.Context, zone, name string) (string, error) {
	f.deleted = append(f.deleted, zone+"/"+name)
	return "operation-delete", f.err
}

type result struct {
	status int
	body   string
}

func (r result) json(t *testing.T) map[string]any {
	t.Helper()
	var body map[string]any
	if err := json.Unmarshal([]byte(r.body), &body); err != nil {
		t.Fatalf("body %q: %v", r.body, err)
	}
	return body
}

func (r result) outcome(t *testing.T) string {
	t.Helper()
	outcome, _ := r.json(t)["outcome"].(string)
	return outcome
}

// failedGuards lists the names of the guards that failed.
func (r result) failedGuards(t *testing.T) []string {
	t.Helper()
	var names []string
	guards, _ := r.json(t)["guards"].([]any)
	for _, g := range guards {
		guard := g.(map[string]any)
		if guard["passed"] == false {
			names = append(names, guard["name"].(string))
		}
	}
	return names
}

func (r result) output(t *testing.T) map[string]any {
	t.Helper()
	out, _ := r.json(t)["result"].(map[string]any)
	return out
}

const nowRFC3339 = "2026-09-30T12:00:00Z"

// fixedClock is a clock stopped at a Unix time.
type fixedClock int64

func (c fixedClock) Now() int64 { return int64(c) }

// failingConnector can't connect to Compute Engine.
type failingConnector struct{ err error }

func (c failingConnector) Connect(context.Context) (compute.Client, error) { return nil, c.err }

func serve(t *testing.T, connector compute.Connector, method, body string) result {
	t.Helper()
	fn := &Function{connector: connector, clock: fixedClock(mustTimestamp(t, nowRFC3339))}
	rec := httptest.NewRecorder()
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(method, "/", strings.NewReader(body)))
	return result{rec.Code, rec.Body.String()}
}

func invokeAs(t *testing.T, fake *fakeCompute, method, body string) result {
	t.Helper()
	return serve(t, fake, method, body)
}

func invoke(t *testing.T, fake *fakeCompute, body string) result {
	t.Helper()
	return invokeAs(t, fake, http.MethodPost, body)
}

func request(action string, extra ...string) string {
	fields := []string{`"zone":"us-central1-a"`, `"disk":"pvc-1"`, `"pvName":"pvc-123"`}
	if action != "" {
		fields = append(fields, `"action":"`+action+`"`)
	}
	return "{" + strings.Join(append(fields, extra...), ",") + "}"
}

func assertBody(t *testing.T, r result, status int, want string) {
	t.Helper()
	if r.status != status || r.body != want {
		t.Errorf("got %d %s\nwant %d %s", r.status, r.body, status, want)
	}
}

func TestPlanDescribesTheDeletion(t *testing.T) {
	fake := &fakeCompute{disk: testDisk(csiDescription)}

	got := invoke(t, fake, request(""))

	assertBody(t, got, http.StatusOK, `{"action":"plan","guards":[`+
		`{"detail":"Volume pvc-1 exists.","name":"exists","passed":true},`+
		`{"detail":"The volume isn't attached to any instance.","name":"unattached","passed":true},`+
		`{"detail":"The volume's state is READY.","name":"state","passed":true},`+
		`{"detail":"The volume was created for PersistentVolume pvc-123 of PersistentVolumeClaim apps/data.","name":"kubernetes","passed":true},`+
		`{"detail":"Execute takes a snapshot first and deletes the volume in a later execute.","name":"snapshot","passed":true}],`+
		`"outcome":"planned","result":{"deleted":false,"snapshot":{"state":"missing"},`+
		`"volume":{"deletable":true,"id":"pvc-1","kubernetes":{"namespace":"apps","pv":"pvc-123","pvc":"data"},"sizeGib":10,"state":"READY"}}}`)
	if len(fake.created)+len(fake.deleted) != 0 {
		t.Errorf("a plan changed something: %+v %v", fake.created, fake.deleted)
	}
	if len(fake.listed) != 1 || fake.listed[0] != "plural-sh-volume-delete=4242" {
		t.Errorf("listed = %v", fake.listed)
	}
}

func TestExecuteStartsSnapshotAndRefuses(t *testing.T) {
	fake := &fakeCompute{disk: testDisk(csiDescription)}

	got := invoke(t, fake, request("execute"))

	assertBody(t, got, http.StatusOK, `{"action":"execute","guards":[`+
		`{"detail":"Volume pvc-1 exists.","name":"exists","passed":true},`+
		`{"detail":"The volume isn't attached to any instance.","name":"unattached","passed":true},`+
		`{"detail":"The volume's state is READY.","name":"state","passed":true},`+
		`{"detail":"The volume was created for PersistentVolume pvc-123 of PersistentVolumeClaim apps/data.","name":"kubernetes","passed":true},`+
		`{"detail":"No snapshot has completed yet.","name":"snapshot","passed":false}],`+
		`"outcome":"refused","result":{"deleted":false,"operation":"operation-snapshot",`+
		`"snapshot":{"id":"pvc-1-predelete-1790769600","state":"inProgress"},`+
		`"volume":{"deletable":true,"id":"pvc-1","kubernetes":{"namespace":"apps","pv":"pvc-123","pvc":"data"},"sizeGib":10,"state":"READY"}}}`)
	want := compute.SnapshotRequest{
		Zone:        "us-central1-a",
		Disk:        "pvc-1",
		Name:        "pvc-1-predelete-1790769600",
		Description: "Taken by Plural before deleting disk pvc-1 in us-central1-a",
		Labels:      map[string]string{"plural-sh-volume-delete": "4242"},
	}
	if len(fake.created) != 1 || !reflect.DeepEqual(fake.created[0], want) {
		t.Errorf("created = %+v, want %+v", fake.created, want)
	}
	if len(fake.deleted) != 0 {
		t.Errorf("deleted = %v", fake.deleted)
	}
}

func TestExecuteRetriesAfterFailedSnapshot(t *testing.T) {
	fake := &fakeCompute{
		disk:      testDisk(csiDescription),
		snapshots: []*compute.Snapshot{testSnapshot("failed", "2026-09-30T11:00:00Z", "FAILED")},
	}

	got := invoke(t, fake, request("execute"))

	if got.outcome(t) != "refused" || len(fake.created) != 1 {
		t.Errorf("got %s, created %v", got.body, fake.created)
	}
}

func TestExecuteDeletesAfterCompletedSnapshot(t *testing.T) {
	fake := &fakeCompute{
		disk:      testDisk(csiDescription),
		snapshots: []*compute.Snapshot{testSnapshot("pvc-1-predelete-1", "2026-09-30T11:00:00Z", "READY")},
	}

	got := invoke(t, fake, request("execute"))

	if got.status != http.StatusOK || got.outcome(t) != "done" || len(got.failedGuards(t)) != 0 {
		t.Fatalf("got %d %s", got.status, got.body)
	}
	out := got.output(t)
	if out["deleted"] != true || out["operation"] != "operation-delete" {
		t.Errorf("result = %v", out)
	}
	if snapshot, _ := out["snapshot"].(map[string]any); snapshot["state"] != "completed" || snapshot["id"] != "pvc-1-predelete-1" {
		t.Errorf("snapshot = %v", out["snapshot"])
	}
	if len(fake.deleted) != 1 || fake.deleted[0] != "us-central1-a/pvc-1" || len(fake.created) != 0 {
		t.Errorf("deleted = %v, created = %v", fake.deleted, fake.created)
	}
}

func TestExecuteWaitsForSnapshotInProgress(t *testing.T) {
	fake := &fakeCompute{
		disk:      testDisk(csiDescription),
		snapshots: []*compute.Snapshot{testSnapshot("pvc-1-predelete-1", "2026-09-30T11:00:00Z", "UPLOADING")},
	}

	got := invoke(t, fake, request("execute"))

	if got.outcome(t) != "refused" || len(got.failedGuards(t)) != 1 || got.output(t)["deleted"] != false {
		t.Errorf("got %s", got.body)
	}
	if !strings.Contains(got.body, "Snapshot pvc-1-predelete-1 is in progress (uploading). Execute again once it completes.") {
		t.Errorf("body = %s", got.body)
	}
	if len(fake.created)+len(fake.deleted) != 0 {
		t.Errorf("created = %v, deleted = %v", fake.created, fake.deleted)
	}
}

func TestExecuteRefusesMissingDisk(t *testing.T) {
	fake := &fakeCompute{}

	got := invoke(t, fake, request("execute"))

	assertBody(t, got, http.StatusOK, `{"action":"execute","guards":[{"detail":"The volume doesn't exist.","name":"exists","passed":false}],`+
		`"outcome":"refused","result":{"deleted":false,"snapshot":{"state":"missing"}}}`)
	if len(fake.listed)+len(fake.created)+len(fake.deleted) != 0 {
		t.Errorf("listed = %v, created = %v, deleted = %v", fake.listed, fake.created, fake.deleted)
	}
}

func TestExecuteRefusesAttachedDiskWithoutSnapshotting(t *testing.T) {
	fake := &fakeCompute{disk: testDisk(csiDescription, instanceURL)}

	got := invoke(t, fake, request("execute"))

	if got.outcome(t) != "refused" || got.failedGuards(t)[0] != "unattached" {
		t.Errorf("got %s", got.body)
	}
	if !strings.Contains(got.body, `"detail":"The volume is attached to node-1."`) {
		t.Errorf("body = %s", got.body)
	}
	if len(fake.created)+len(fake.deleted) != 0 {
		t.Errorf("created = %v, deleted = %v", fake.created, fake.deleted)
	}
}

func TestExecuteRefusesDiskOfAnotherPersistentVolume(t *testing.T) {
	fake := &fakeCompute{disk: testDisk(csiDescription)}

	got := invoke(t, fake, `{"zone":"us-central1-a","disk":"pvc-1","pvName":"pvc-999","action":"execute"}`)

	if got.outcome(t) != "refused" || strings.Join(got.failedGuards(t), ",") != "kubernetes,snapshot" {
		t.Errorf("got %s", got.body)
	}
	if len(fake.created)+len(fake.deleted) != 0 {
		t.Errorf("created = %v, deleted = %v", fake.created, fake.deleted)
	}
}

func TestSkippingTheSnapshotIsRejectedUnlessAllowed(t *testing.T) {
	t.Setenv(volume.AllowSkipSnapshotVar, "")
	fake := &fakeCompute{disk: testDisk(csiDescription)}

	got := invoke(t, fake, request("execute", `"snapshot":false`))

	assertBody(t, got, http.StatusBadRequest, `{"errorMessage":"invalid request: snapshot: false is not allowed by this installation, `+
		`which always snapshots the volume first","errorType":"InvalidRequest"}`)
	if len(fake.deleted) != 0 {
		t.Errorf("deleted = %v", fake.deleted)
	}
}

func TestSkippingTheSnapshotWhenAllowed(t *testing.T) {
	t.Setenv(volume.AllowSkipSnapshotVar, "true")
	fake := &fakeCompute{disk: testDisk(csiDescription)}

	got := invoke(t, fake, request("execute", `"snapshot":false`))

	if got.outcome(t) != "done" || got.output(t)["deleted"] != true {
		t.Fatalf("got %s", got.body)
	}
	if _, ok := got.output(t)["snapshot"]; ok {
		t.Errorf("snapshot should be omitted: %s", got.body)
	}
	if len(fake.listed) != 0 || len(fake.deleted) != 1 {
		t.Errorf("listed = %v, deleted = %v", fake.listed, fake.deleted)
	}
}

func TestRejectsInvalidParameters(t *testing.T) {
	tests := []struct {
		name string
		body string
		want string
	}{
		{"invalid zone", `{"zone":"nope","disk":"pvc-1","pvName":"pvc-1"}`,
			`invalid request: zone \"nope\" is not a Compute Engine zone (e.g. us-central1-a)`},
		{"invalid disk", `{"zone":"us-central1-a","disk":"Disk_1","pvName":"pvc-1"}`,
			`invalid request: disk \"Disk_1\" is not a Compute Engine disk name`},
		{"invalid PV name", `{"zone":"us-central1-a","disk":"pvc-1","pvName":"PV"}`,
			`invalid request: pvName \"PV\" is not a PersistentVolume name`},
		{"missing zone", `{"disk":"pvc-1","pvName":"pvc-1"}`, "invalid request: missing field `zone`"},
		{"missing disk", `{"zone":"us-central1-a","pvName":"pvc-1"}`, "invalid request: missing field `disk`"},
		{"missing PV name", `{"zone":"us-central1-a","disk":"pvc-1"}`, "invalid request: missing field `pvName`"},
		{"unknown action", `{"action":"delete","zone":"us-central1-a","disk":"pvc-1","pvName":"pvc-1"}`,
			"invalid request: unknown variant `delete`, expected `plan` or `execute`"},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			fake := &fakeCompute{disk: testDisk(csiDescription)}

			got := invoke(t, fake, tt.body)

			want := `{"errorMessage":"` + tt.want + `","errorType":"InvalidRequest"}`
			assertBody(t, got, http.StatusBadRequest, want)
			if len(fake.listed)+len(fake.created)+len(fake.deleted) != 0 {
				t.Error("the request reached Compute Engine")
			}
		})
	}
}

func TestRejectsInvalidJSON(t *testing.T) {
	for _, body := range []string{"", "{", "[]", "null", "not json"} {
		got := invoke(t, &fakeCompute{}, body)

		if got.status != http.StatusBadRequest || got.json(t)["errorType"] != "InvalidRequest" {
			t.Errorf("%q: got %d %s", body, got.status, got.body)
		}
	}
}

func TestRejectsOtherMethods(t *testing.T) {
	for _, method := range []string{http.MethodGet, http.MethodPut, http.MethodDelete} {
		got := invokeAs(t, &fakeCompute{}, method, "")

		if got.status != http.StatusMethodNotAllowed || got.body != "" {
			t.Errorf("%s: got %d %q", method, got.status, got.body)
		}
	}
}

func TestMapsComputeErrorsToProviderErrors(t *testing.T) {
	fake := &fakeCompute{disk: testDisk(csiDescription), err: core.Providerf("compute API: permission denied")}

	got := invoke(t, fake, request("execute"))

	assertBody(t, got, http.StatusBadGateway, `{"errorMessage":"cloud provider error: compute API: permission denied","errorType":"Provider"}`)
}

func TestConnectErrorsAreProviderErrors(t *testing.T) {
	connector := failingConnector{core.Providerf("GOOGLE_CLOUD_PROJECT is not set")}

	got := serve(t, connector, http.MethodPost, request(""))

	if got.status != http.StatusBadGateway || !strings.Contains(got.body, "GOOGLE_CLOUD_PROJECT is not set") {
		t.Errorf("got %d %s", got.status, got.body)
	}
}

func TestRejectsDiskWithoutID(t *testing.T) {
	resource := testDisk(csiDescription)
	resource.Id = nil

	got := invoke(t, &fakeCompute{disk: resource}, request(""))

	assertBody(t, got, http.StatusBadGateway, `{"errorMessage":"cloud provider error: compute API returned a disk without an ID","errorType":"Provider"}`)
}
