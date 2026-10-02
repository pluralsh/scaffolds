package vmdelete

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"slices"
	"strconv"
	"strings"
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

// fakeCompute is a Compute Engine that answers instance reads in order, repeating the last
// one, and records what is asked of it.
type fakeCompute struct {
	// Disks, groups, access and load balancers aren't used by this function; calling them panics.
	compute.Disks
	compute.Groups
	compute.Access
	compute.LoadBalancers

	instances []*compute.Instance
	err       error

	reads      int
	autoDelete []string
	deleted    []string
}

func (f *fakeCompute) Connect(context.Context) (compute.Client, error) { return f, nil }

func (f *fakeCompute) Instance(_ context.Context, _, _ string) (*compute.Instance, error) {
	if f.err != nil || len(f.instances) == 0 {
		return nil, f.err
	}
	instance := f.instances[min(f.reads, len(f.instances)-1)]
	f.reads++
	return instance, nil
}

func (f *fakeCompute) SetDiskAutoDelete(_ context.Context, zone, instance, deviceName string, autoDelete bool) (string, error) {
	f.autoDelete = append(f.autoDelete, zone+"/"+instance+"/"+deviceName+"="+strconv.FormatBool(autoDelete))
	return "operation-auto-delete", nil
}

func (f *fakeCompute) DeleteInstance(_ context.Context, zone, name string) (string, error) {
	f.deleted = append(f.deleted, zone+"/"+name)
	return "operation-delete", nil
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

func invoke(t *testing.T, fake *fakeCompute, body string) result {
	t.Helper()
	rec := httptest.NewRecorder()
	fn := &Function{connector: fake}
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body)))
	return result{rec.Code, rec.Body.String()}
}

func request(action string) string {
	return `{"action":"` + action + `","zone":"us-central1-a","instance":"vm-1"}`
}

func assertBody(t *testing.T, r result, status int, want string) {
	t.Helper()
	if r.status != status || r.body != want {
		t.Errorf("got %d %s\nwant %d %s", r.status, r.body, status, want)
	}
}

func (f *fakeCompute) assertNoWrites(t *testing.T) {
	t.Helper()
	if len(f.autoDelete)+len(f.deleted) != 0 {
		t.Errorf("changed something: auto-delete %v, deleted %v", f.autoDelete, f.deleted)
	}
}

func TestPlanDescribesTheDeletion(t *testing.T) {
	instance := testInstance()
	instance.Disks = append(instance.Disks, testLocalSSD("local-ssd-0"))
	fake := &fakeCompute{instances: []*compute.Instance{instance}}

	got := invoke(t, fake, request("plan"))

	assertBody(t, got, http.StatusOK, `{"action":"plan","guards":[`+
		`{"detail":"instance vm-1","name":"exists","passed":true},`+
		`{"detail":"not part of a managed instance group","name":"standalone","passed":true},`+
		`{"detail":"not managed by GKE","name":"not-gke","passed":true},`+
		`{"detail":"deletion protection is disabled","name":"deletion-protection","passed":true},`+
		`{"detail":"the boot disk must be a persistent disk to be deleted with the instance","name":"boot-disk","passed":true},`+
		`{"detail":"status RUNNING","name":"idle","passed":true}],`+
		`"outcome":"planned","result":{"autoDeleteSet":false,"deleted":false,`+
		`"instance":{"bootDisk":"vm-1","dataDisks":["vm-1-data"],"localSsds":["local-ssd-0"],"name":"vm-1","state":"RUNNING"}}}`)
	fake.assertNoWrites(t)
}

func TestPlanOfMissingInstance(t *testing.T) {
	fake := &fakeCompute{}

	got := invoke(t, fake, request("plan"))

	assertBody(t, got, http.StatusOK, `{"action":"plan","guards":[`+
		`{"detail":"instance not found","name":"exists","passed":false}],`+
		`"outcome":"refused","result":{"autoDeleteSet":false,"deleted":false}}`)
}

func TestRefusesUnsafeInstancesWithoutChangingThem(t *testing.T) {
	for name, tc := range map[string]struct {
		instance *compute.Instance
		failed   []string
	}{
		"managed instance group": {
			withMetadata(testInstance(), createdByKey, "projects/1/zones/us-central1-a/instanceGroupManagers/web"),
			[]string{guardStandalone},
		},
		"GKE node": {
			func() *compute.Instance {
				i := withMetadata(testInstance(), createdByKey, "projects/1/zones/us-central1-a/instanceGroupManagers/gke-c-pool")
				i.Labels = map[string]string{gkeNodeLabel: "", gkeClusterLabel: "c"}
				return i
			}(),
			[]string{guardStandalone, guardNotGKE},
		},
		"deletion protection": {
			func() *compute.Instance { i := testInstance(); i.DeletionProtection = new(true); return i }(),
			[]string{guardDeletionProtection},
		},
		"busy": {withStatus(testInstance(), "STOPPING"), []string{guardIdle}},
		"no boot disk": {
			func() *compute.Instance { i := testInstance(); i.Disks = i.Disks[1:]; return i }(),
			[]string{guardBootDisk},
		},
	} {
		t.Run(name, func(t *testing.T) {
			fake := &fakeCompute{instances: []*compute.Instance{tc.instance}}

			got := invoke(t, fake, request("execute"))

			if got.outcome(t) != "refused" || !slices.Equal(got.failedGuards(t), tc.failed) {
				t.Errorf("got %s, want refused with %v failed", got.body, tc.failed)
			}
			fake.assertNoWrites(t)
		})
	}
}

// The usual sequence: Compute Engine applies the flags after answering, so the first execute
// sets them and refuses, and the next one deletes the instance.
func TestExecuteSetsAutoDeleteThenDeletesInALaterExecute(t *testing.T) {
	instance := testInstance()
	instance.Disks = append(instance.Disks, testLocalSSD("local-ssd-0"))
	fake := &fakeCompute{instances: []*compute.Instance{instance, instance}}

	first := invoke(t, fake, request("execute"))

	if first.outcome(t) != "refused" || !slices.Equal(first.failedGuards(t), []string{guardAutoDelete}) {
		t.Errorf("first execute: %s", first.body)
	}
	if out := first.json(t)["result"].(map[string]any); out["autoDeleteSet"] != true || out["deleted"] != false {
		t.Errorf("first result = %v", out)
	}
	// The boot disk is set to be deleted and the data disk kept; the local SSD is left alone.
	want := []string{"us-central1-a/vm-1/vm-1=true", "us-central1-a/vm-1/vm-1-data=false"}
	if !slices.Equal(fake.autoDelete, want) || len(fake.deleted) != 0 {
		t.Errorf("auto-delete %v, deleted %v", fake.autoDelete, fake.deleted)
	}

	fake.instances = []*compute.Instance{withAutoDelete(testInstance())}
	fake.reads = 0
	second := invoke(t, fake, request("execute"))

	if second.outcome(t) != "done" || !slices.Equal(fake.deleted, []string{"us-central1-a/vm-1"}) {
		t.Errorf("second execute: %s, deleted %v", second.body, fake.deleted)
	}
	if len(fake.autoDelete) != 2 {
		t.Errorf("flags set again: %v", fake.autoDelete)
	}
}

// The uncommon case where the flags are already applied when the instance is read again.
func TestExecuteSetsAutoDeleteAndDeletesInOneExecute(t *testing.T) {
	fake := &fakeCompute{instances: []*compute.Instance{testInstance(), withAutoDelete(testInstance())}}

	got := invoke(t, fake, request("execute"))

	assertBody(t, got, http.StatusOK, `{"action":"execute","guards":[`+
		`{"detail":"instance vm-1","name":"exists","passed":true},`+
		`{"detail":"not part of a managed instance group","name":"standalone","passed":true},`+
		`{"detail":"not managed by GKE","name":"not-gke","passed":true},`+
		`{"detail":"deletion protection is disabled","name":"deletion-protection","passed":true},`+
		`{"detail":"the boot disk must be a persistent disk to be deleted with the instance","name":"boot-disk","passed":true},`+
		`{"detail":"status RUNNING","name":"idle","passed":true}],`+
		`"outcome":"done","result":{"autoDeleteSet":true,"deleted":true,`+
		`"instance":{"bootDisk":"vm-1","dataDisks":["vm-1-data"],"name":"vm-1","state":"RUNNING"},"operation":"operation-delete"}}`)
}

func TestExecuteDeletesDirectlyOnceAutoDeleteIsSet(t *testing.T) {
	fake := &fakeCompute{instances: []*compute.Instance{withAutoDelete(testInstance())}}

	got := invoke(t, fake, request("execute"))

	if got.outcome(t) != "done" || len(fake.autoDelete) != 0 || fake.reads != 1 {
		t.Errorf("got %s, auto-delete %v, reads %d", got.body, fake.autoDelete, fake.reads)
	}
}

func TestExecuteOfStoppedInstance(t *testing.T) {
	fake := &fakeCompute{instances: []*compute.Instance{withStatus(withAutoDelete(testInstance()), "TERMINATED")}}

	got := invoke(t, fake, request("execute"))

	if got.outcome(t) != "done" {
		t.Errorf("got %s", got.body)
	}
}

func TestProviderErrorsAreReported(t *testing.T) {
	fake := &fakeCompute{err: core.Providerf("compute API: boom")}

	got := invoke(t, fake, request("plan"))

	assertBody(t, got, http.StatusBadGateway, `{"errorMessage":"cloud provider error: compute API: boom","errorType":"Provider"}`)
}

func TestConnectionErrorsAreReported(t *testing.T) {
	rec := httptest.NewRecorder()
	fn := &Function{connector: failingConnector{errors.New("no credentials")}}
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(request("plan"))))

	if rec.Code != http.StatusBadGateway {
		t.Errorf("got %d %s", rec.Code, rec.Body)
	}
}

// failingConnector can't connect to Compute Engine.
type failingConnector struct{ err error }

func (c failingConnector) Connect(context.Context) (compute.Client, error) { return nil, c.err }

func TestInvalidParamsAreRejectedBeforeConnecting(t *testing.T) {
	got := invoke(t, nil, `{"action":"plan","zone":"us-central1-a","instance":"VM_1"}`)

	assertBody(t, got, http.StatusBadRequest,
		`{"errorMessage":"invalid request: instance \"VM_1\" is not a Compute Engine instance name","errorType":"InvalidRequest"}`)
}
