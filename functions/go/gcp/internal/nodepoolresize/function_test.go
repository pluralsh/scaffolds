package nodepoolresize

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"slices"
	"strconv"
	"strings"
	"testing"

	container "google.golang.org/api/container/v1"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gke"
)

const groupURL = "https://www.googleapis.com/compute/v1/projects/p/zones/%s/instanceGroupManagers/gke-c-apps-grp"

// fakeCloud is GKE and Compute Engine: it answers with the cluster and the instance group
// sizes it holds, and records the resizes asked of it.
type fakeCloud struct {
	// Disks, instances, access and load balancers aren't used by this function; calling them panics.
	compute.Disks
	compute.Instances
	compute.Access
	compute.LoadBalancers

	cluster *gke.Cluster
	sizes   map[string]int32
	err     error

	resized []string
}

func (f *fakeCloud) connectGKE() gke.Connector                       { return gkeConnector{f} }
func (f *fakeCloud) Connect(context.Context) (compute.Client, error) { return f, nil }

type gkeConnector struct{ f *fakeCloud }

func (c gkeConnector) Connect(context.Context) (gke.Client, error) { return c.f, nil }

func (f *fakeCloud) Cluster(context.Context, string, string) (*gke.Cluster, error) {
	return f.cluster, f.err
}

func (f *fakeCloud) SetNodePoolSize(_ context.Context, location, cluster, pool string, count int64) (string, error) {
	f.resized = append(f.resized, location+"/"+cluster+"/"+pool+"="+strconv.FormatInt(count, 10))
	return "operation-resize", nil
}

func (f *fakeCloud) InstanceGroupManager(_ context.Context, zone, _ string) (*compute.InstanceGroupManager, error) {
	size, ok := f.sizes[zone]
	if !ok {
		return nil, nil
	}
	return &compute.InstanceGroupManager{TargetSize: &size}, nil
}

// testCluster is a running regional cluster with a manually scaled pool `apps` in two zones,
// with two nodes in each, and a `system` pool.
func testCluster() *gke.Cluster {
	zones := []string{"us-central1-a", "us-central1-b"}
	return &gke.Cluster{
		Name:      "c",
		Status:    "RUNNING",
		Locations: zones,
		NodePools: []*gke.NodePool{
			{
				Name:              "apps",
				Status:            "RUNNING",
				Config:            &container.NodeConfig{MachineType: "e2-standard-4"},
				InstanceGroupUrls: []string{strings.Replace(groupURL, "%s", zones[0], 1), strings.Replace(groupURL, "%s", zones[1], 1)},
			},
			{Name: "system", Status: "RUNNING"},
		},
	}
}

func newFake() *fakeCloud {
	return &fakeCloud{cluster: testCluster(), sizes: map[string]int32{"us-central1-a": 2, "us-central1-b": 2}}
}

func invoke(t *testing.T, fake *fakeCloud, body string) (int, string) {
	t.Helper()
	rec := httptest.NewRecorder()
	fn := &Function{gke: fake.connectGKE(), compute: fake, maxCount: 10}
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body)))
	return rec.Code, rec.Body.String()
}

func request(action string, count int) string {
	body, _ := json.Marshal(map[string]any{"action": action, "location": "us-central1", "cluster": "c", "nodePool": "apps", "count": count})
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

func TestPlanDescribesTheResize(t *testing.T) {
	fake := newFake()

	status, body := invoke(t, fake, request("plan", 3))

	want := `{"action":"plan","guards":[` +
		`{"detail":"node pool apps of cluster c","name":"exists","passed":true},` +
		`{"detail":"Standard cluster; Autopilot clusters manage their nodes themselves","name":"standard-cluster","passed":true},` +
		`{"detail":"cluster RUNNING, node pool RUNNING; another operation must finish first unless both are RUNNING","name":"idle","passed":true},` +
		`{"detail":"cluster autoscaler disabled","name":"manually-scaled","passed":true},` +
		`{"detail":"3 nodes per zone in 2 zones, 6 in total; the installation allows at most 10 in total","name":"count-allowed","passed":true}],` +
		`"outcome":"planned","result":{"from":4,"nodePool":{"autoscaling":false,"machineType":"e2-standard-4","name":"apps",` +
		`"nodesPerZone":{"us-central1-a":2,"us-central1-b":2},"state":"RUNNING","zones":["us-central1-a","us-central1-b"]},` +
		`"submitted":false,"to":6}}`
	if status != http.StatusOK || body != want {
		t.Errorf("got %d %s\nwant %s", status, body, want)
	}
	if len(fake.resized) != 0 {
		t.Errorf("resized on plan: %v", fake.resized)
	}
}

func TestExecuteResizes(t *testing.T) {
	fake := newFake()

	_, body := invoke(t, fake, request("execute", 3))

	if got, failed := outcome(t, body); got != "done" || len(failed) != 0 {
		t.Errorf("got %s", body)
	}
	if !slices.Equal(fake.resized, []string{"us-central1/c/apps=3"}) || !strings.Contains(body, `"operation":"operation-resize","submitted":true`) {
		t.Errorf("resized %v, body %s", fake.resized, body)
	}
}

func TestExecuteOfUnchangedCountSubmitsNothing(t *testing.T) {
	fake := newFake()

	_, body := invoke(t, fake, request("execute", 2))

	if got, _ := outcome(t, body); got != "done" || len(fake.resized) != 0 || !strings.Contains(body, `"submitted":false`) {
		t.Errorf("got %s, resized %v", body, fake.resized)
	}
}

func TestExecuteResizesWhenOnlyOneZoneDiffers(t *testing.T) {
	fake := newFake()
	fake.sizes["us-central1-b"] = 1

	_, body := invoke(t, fake, request("execute", 2))

	if got, _ := outcome(t, body); got != "done" || len(fake.resized) != 1 || !strings.Contains(body, `"from":3`) {
		t.Errorf("got %s, resized %v", body, fake.resized)
	}
}

func TestRefusesUnsafeResizesWithoutChangingAnything(t *testing.T) {
	for name, tc := range map[string]struct {
		change func(*gke.Cluster)
		count  int
		failed []string
	}{
		"autopilot":    {func(c *gke.Cluster) { c.Autopilot = &container.Autopilot{Enabled: true} }, 3, []string{guardStandard}},
		"cluster busy": {func(c *gke.Cluster) { c.Status = "RECONCILING" }, 3, []string{guardIdle}},
		"pool busy":    {func(c *gke.Cluster) { c.NodePools[0].Status = "PROVISIONING" }, 3, []string{guardIdle}},
		"autoscaled": {func(c *gke.Cluster) {
			c.NodePools[0].Autoscaling = &container.NodePoolAutoscaling{Enabled: true, MaxNodeCount: 5}
		}, 3, []string{guardManuallyScaled}},
		"over the cap":  {func(*gke.Cluster) {}, 6, []string{guardCountAllowed}},
		"only pool":     {func(c *gke.Cluster) { c.NodePools = c.NodePools[:1] }, 0, []string{guardCountAllowed}},
		"missing pool":  {func(c *gke.Cluster) { c.NodePools = c.NodePools[1:] }, 3, []string{guardExists}},
		"other pool ok": {func(*gke.Cluster) {}, 0, nil},
	} {
		t.Run(name, func(t *testing.T) {
			fake := newFake()
			tc.change(fake.cluster)

			_, body := invoke(t, fake, request("execute", tc.count))

			got, failed := outcome(t, body)
			if tc.failed == nil {
				if got != "done" {
					t.Errorf("got %s", body)
				}
				return
			}
			if got != "refused" || !slices.Equal(failed, tc.failed) || len(fake.resized) != 0 {
				t.Errorf("got %s, resized %v, want refused with %v failed", body, fake.resized, tc.failed)
			}
		})
	}
}

func TestPlanOfMissingCluster(t *testing.T) {
	fake := newFake()
	fake.cluster = nil

	_, body := invoke(t, fake, request("plan", 3))

	want := `{"action":"plan","guards":[{"detail":"cluster or node pool not found","name":"exists","passed":false}],"outcome":"refused","result":{"submitted":false,"to":3}}`
	if body != want {
		t.Errorf("got %s", body)
	}
}

func TestUnknownZoneSizesLeaveFromOut(t *testing.T) {
	fake := newFake()
	delete(fake.sizes, "us-central1-b")

	_, body := invoke(t, fake, request("plan", 3))

	if strings.Contains(body, `"from"`) {
		t.Errorf("got %s", body)
	}
}

func TestProviderErrorsAreReported(t *testing.T) {
	fake := newFake()
	fake.err = core.Providerf("GKE API: boom")

	status, body := invoke(t, fake, request("plan", 3))

	if status != http.StatusBadGateway || body != `{"errorMessage":"cloud provider error: GKE API: boom","errorType":"Provider"}` {
		t.Errorf("got %d %s", status, body)
	}
}

func TestInvalidParamsAreRejected(t *testing.T) {
	for name, body := range map[string]string{
		"location": `{"location":"us","cluster":"c","nodePool":"apps","count":1}`,
		"cluster":  `{"location":"us-central1","cluster":"C","nodePool":"apps","count":1}`,
		"nodePool": `{"location":"us-central1","cluster":"c","nodePool":"Apps","count":1}`,
		"count":    `{"location":"us-central1","cluster":"c","nodePool":"apps","count":-1}`,
	} {
		status, got := invoke(t, nil, body)

		if status != http.StatusBadRequest || !strings.Contains(got, name) {
			t.Errorf("%s: got %d %s", name, status, got)
		}
	}
}

func TestMaxCountFromEnv(t *testing.T) {
	for value, want := range map[string]int64{"": gkeMaxCount, "50": 50, "5000": gkeMaxCount, "x": gkeMaxCount, "-1": gkeMaxCount} {
		t.Setenv(MaxCountVar, value)

		if got := MaxCountFromEnv(); got != want {
			t.Errorf("%q: got %d, want %d", value, got, want)
		}
	}
}
