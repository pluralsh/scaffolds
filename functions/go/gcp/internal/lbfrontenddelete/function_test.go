package lbfrontenddelete

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"slices"
	"strings"
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const (
	lb      = "a0123456789abcdef0123456789abcde"
	service = "default/web"
	owned   = `{"kubernetes.io/service-name":"default/web"}`
	base    = "https://www.googleapis.com/compute/v1/projects/p/"
)

// fakeLB is Compute Engine with the resources of one load balancer: it answers reads from
// them, reports the healthy members, and deletes resources when asked unless they are busy.
type fakeLB struct {
	// Only load balancers are used by this function; calling anything else panics.
	compute.Disks
	compute.Instances
	compute.Groups
	compute.Access

	resources map[string]*compute.LBResource // by kind/name
	rules     []*compute.LBResource          // other forwarding rules of the region
	healthy   []string
	healthErr error
	busy      []string

	deleted []string
}

func (f *fakeLB) Connect(context.Context) (compute.Client, error) { return f, nil }

func (f *fakeLB) LBResource(_ context.Context, kind compute.LBKind, _, name string) (*compute.LBResource, error) {
	return f.resources[string(kind)+"/"+name], nil
}

func (f *fakeLB) ForwardingRules(context.Context, string) ([]*compute.LBResource, error) {
	return f.rules, nil
}

func (f *fakeLB) Healthy(_ context.Context, _ *compute.LBResource, member string) (bool, error) {
	return slices.Contains(f.healthy, member), f.healthErr
}

func (f *fakeLB) DeleteLBResource(_ context.Context, kind compute.LBKind, _, name string) (string, error) {
	key := string(kind) + "/" + name
	if slices.Contains(f.busy, key) {
		return "", fmt.Errorf("%w: in use", compute.ErrBusy)
	}
	f.deleted = append(f.deleted, key)
	delete(f.resources, key)
	return "operation-" + name, nil
}

func (f *fakeLB) add(r *compute.LBResource) *compute.LBResource {
	if r.Description == "" {
		r.Description = owned
	}
	if r.SelfLink == "" {
		r.SelfLink = base + "regions/us-central1/" + string(r.Kind) + "s/" + r.Name
	}
	f.resources[string(r.Kind)+"/"+r.Name] = r
	return r
}

// newFake is what the cloud provider leaves of an external load balancer of a Service with
// externalTrafficPolicy Local: forwarding rule, target pool with two nodes, its health check,
// two firewall rules and a reserved address.
func newFake() *fakeLB {
	f := &fakeLB{resources: map[string]*compute.LBResource{}}
	pool := f.add(&compute.LBResource{Kind: compute.KindTargetPool, Name: lb, Members: []string{base + "zones/us-central1-a/instances/node-1", base + "zones/us-central1-b/instances/node-2"}})
	f.add(&compute.LBResource{Kind: compute.KindForwardingRule, Name: lb, Target: pool.SelfLink, IPAddress: "1.2.3.4"})
	f.add(&compute.LBResource{Kind: compute.KindHTTPHealthCheck, Name: lb})
	f.add(&compute.LBResource{Kind: compute.KindFirewall, Name: "k8s-fw-" + lb, Description: `{"kubernetes.io/service-name":"default/web", "kubernetes.io/service-ip":"1.2.3.4"}`})
	f.add(&compute.LBResource{Kind: compute.KindFirewall, Name: "k8s-" + lb + "-http-hc"})
	f.add(&compute.LBResource{Kind: compute.KindAddress, Name: lb, Status: "IN_USE", Users: []string{base + "regions/us-central1/forwardingRules/" + lb}})
	return f
}

func invoke(t *testing.T, fake *fakeLB, body string) (int, string) {
	t.Helper()
	rec := httptest.NewRecorder()
	fn := &Function{connector: fake}
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body)))
	return rec.Code, rec.Body.String()
}

func request(action string) string {
	return `{"action":"` + action + `","region":"us-central1","forwardingRule":"` + lb + `","serviceName":"` + service + `"}`
}

type response struct {
	Outcome string
	Guards  []core.Guard
	Result  Output
}

func decode(t *testing.T, body string) response {
	t.Helper()
	var resp response
	if err := json.Unmarshal([]byte(body), &resp); err != nil {
		t.Fatalf("body %q: %v", body, err)
	}
	return resp
}

func (r response) failed() []string {
	var names []string
	for _, g := range r.Guards {
		if !g.Passed {
			names = append(names, g.Name)
		}
	}
	return names
}

// steps lists kind/name=step for each resource.
func (r response) steps() []string {
	var steps []string
	for _, res := range r.Result.Resources {
		steps = append(steps, string(res.Kind)+"/"+strings.Replace(res.Name, lb, "LB", 1)+"="+string(res.Step))
	}
	return steps
}

func TestPlanDescribesTheFirstStep(t *testing.T) {
	fake := newFake()

	status, body := invoke(t, fake, request("plan"))

	resp := decode(t, body)
	want := []string{
		"forwardingRule/LB=delete", "targetPool/LB=later", "firewall/k8s-fw-LB=later",
		"firewall/k8s-LB-http-hc=later", "address/LB=later", "httpHealthCheck/LB=later",
	}
	if status != http.StatusOK || resp.Outcome != "planned" || !slices.Equal(resp.steps(), want) || !resp.Result.Remaining {
		t.Errorf("got %d %s\nsteps %v", status, body, resp.steps())
	}
	if len(fake.deleted) != 0 {
		t.Errorf("deleted on plan: %v", fake.deleted)
	}
	if !strings.Contains(body, `{"detail":"The forwarding rule was created for Service default/web.","name":"service","passed":true}`) ||
		!strings.Contains(body, `{"detail":"targetPool `+lb+` has no healthy backends.","name":"no-healthy-backends","passed":true}`) {
		t.Errorf("guards: %s", body)
	}
}

// Each execute deletes what nothing uses any more, until nothing is left.
func TestExecutesDeleteEverythingInOrder(t *testing.T) {
	fake := newFake()

	first := decode(t, func() string { _, b := invoke(t, fake, request("execute")); return b }())
	if first.Outcome != "done" || !slices.Equal(fake.deleted, []string{"forwardingRule/" + lb}) || !first.Result.Remaining {
		t.Fatalf("first: %+v, deleted %v", first, fake.deleted)
	}
	// Compute Engine releases the address once the forwarding rule is gone.
	fake.resources["address/"+lb].Status, fake.resources["address/"+lb].Users = "RESERVED", nil

	second := decode(t, func() string { _, b := invoke(t, fake, request("execute")); return b }())
	want := []string{"forwardingRule/" + lb, "targetPool/" + lb, "firewall/k8s-fw-" + lb, "firewall/k8s-" + lb + "-http-hc", "address/" + lb}
	if second.Outcome != "done" || !slices.Equal(fake.deleted, want) || !second.Result.Remaining {
		t.Fatalf("second: %+v, deleted %v", second, fake.deleted)
	}

	third := decode(t, func() string { _, b := invoke(t, fake, request("execute")); return b }())
	if third.Outcome != "done" || fake.deleted[len(fake.deleted)-1] != "httpHealthCheck/"+lb || third.Result.Remaining {
		t.Fatalf("third: %+v, deleted %v", third, fake.deleted)
	}

	fourth := decode(t, func() string { _, b := invoke(t, fake, request("execute")); return b }())
	if fourth.Outcome != "refused" || !slices.Equal(fourth.failed(), []string{guardExists}) {
		t.Errorf("fourth: %+v", fourth)
	}
}

func TestBusyResourcesAreRetriedLater(t *testing.T) {
	fake := newFake()
	delete(fake.resources, "forwardingRule/"+lb)
	fake.busy = []string{"targetPool/" + lb}

	_, body := invoke(t, fake, request("execute"))

	resp := decode(t, body)
	if resp.Outcome != "done" || !resp.Result.Remaining || !slices.Contains(resp.steps(), "targetPool/LB=waiting") {
		t.Errorf("got %s", body)
	}
}

func TestRefusesUnsafeDeletionsWithoutDeletingAnything(t *testing.T) {
	for name, tc := range map[string]struct {
		change func(*fakeLB)
		failed []string
	}{
		"healthy backend": {func(f *fakeLB) { f.healthy = []string{base + "zones/us-central1-b/instances/node-2"} }, []string{guardUnhealthy}},
		"other service": {func(f *fakeLB) {
			f.resources["forwardingRule/"+lb].Description = `{"kubernetes.io/service-name":"default/api"}`
		}, []string{guardService}},
		"no service": {func(f *fakeLB) { f.resources["forwardingRule/"+lb].Description = "" }, []string{guardService}},
		"not a service load balancer": {func(f *fakeLB) {
			f.resources["forwardingRule/"+lb].Target = base + "regions/us-central1/targetHttpProxies/x"
		}, []string{guardTarget}},
		"nothing left": {func(f *fakeLB) { clear(f.resources) }, []string{guardExists}},
	} {
		t.Run(name, func(t *testing.T) {
			fake := newFake()
			tc.change(fake)

			_, body := invoke(t, fake, request("execute"))

			if resp := decode(t, body); resp.Outcome != "refused" || !slices.Equal(resp.failed(), tc.failed) || len(fake.deleted) != 0 {
				t.Errorf("got %s, deleted %v, want refused with %v failed", body, fake.deleted, tc.failed)
			}
		})
	}
}

func TestKeepsResourcesNotCreatedForTheService(t *testing.T) {
	fake := newFake()
	delete(fake.resources, "forwardingRule/"+lb)
	fake.resources["address/"+lb].Description = `{"kubernetes.io/service-name":"default/api"}`
	fake.resources["firewall/k8s-fw-"+lb].Description = "managed by hand"

	_, body := invoke(t, fake, request("execute"))

	resp := decode(t, body)
	if !slices.Contains(resp.steps(), "address/LB=keep") || !slices.Contains(resp.steps(), "firewall/k8s-fw-LB=keep") ||
		slices.Contains(fake.deleted, "address/"+lb) || slices.Contains(fake.deleted, "firewall/k8s-fw-"+lb) {
		t.Errorf("got %s, deleted %v", body, fake.deleted)
	}
}

func TestKeepsTargetsOtherForwardingRulesUse(t *testing.T) {
	fake := newFake()
	delete(fake.resources, "forwardingRule/"+lb)
	fake.rules = []*compute.LBResource{{Name: "other", Target: fake.resources["targetPool/"+lb].SelfLink}}

	_, body := invoke(t, fake, request("execute"))

	resp := decode(t, body)
	if !slices.Contains(resp.steps(), "targetPool/LB=keep") || !slices.Contains(resp.steps(), "httpHealthCheck/LB=keep") ||
		slices.Contains(fake.deleted, "targetPool/"+lb) || resp.Result.Remaining {
		t.Errorf("got %s, deleted %v", body, fake.deleted)
	}
}

func TestInternalLoadBalancers(t *testing.T) {
	fake := &fakeLB{resources: map[string]*compute.LBResource{}}
	service := fake.add(&compute.LBResource{Kind: compute.KindBackendService, Name: lb, Members: []string{base + "zones/us-central1-a/instanceGroups/k8s-ig--c"}})
	fake.add(&compute.LBResource{Kind: compute.KindForwardingRule, Name: lb, Target: service.SelfLink})
	fake.add(&compute.LBResource{Kind: compute.KindHealthCheck, Name: lb})
	fake.healthy = service.Members

	_, body := invoke(t, fake, request("plan"))

	if resp := decode(t, body); !slices.Equal(resp.failed(), []string{guardUnhealthy}) ||
		!strings.Contains(body, "backendService "+lb+" reports k8s-ig--c as healthy") {
		t.Errorf("got %s", body)
	}
}

func TestInvalidParamsAreRejected(t *testing.T) {
	for name, body := range map[string]string{
		"region":         `{"region":"us-central1-a","forwardingRule":"` + lb + `","serviceName":"default/web"}`,
		"forwardingRule": `{"region":"us-central1","forwardingRule":"k8s2-tcp-abc","serviceName":"default/web"}`,
		"serviceName":    `{"region":"us-central1","forwardingRule":"` + lb + `","serviceName":"web"}`,
	} {
		status, got := invoke(t, nil, body)

		if status != http.StatusBadRequest || !strings.Contains(got, name+" ") {
			t.Errorf("%s: got %d %s", name, status, got)
		}
	}
}

func TestFindsAHealthyBackendAmongMany(t *testing.T) {
	fake := newFake()
	pool := fake.resources["targetPool/"+lb]
	pool.Members = nil
	for i := range 40 {
		pool.Members = append(pool.Members, fmt.Sprintf("%szones/us-central1-a/instances/node-%d", base, i))
	}
	fake.healthy = []string{pool.Members[37]}

	_, body := invoke(t, fake, request("execute"))

	if resp := decode(t, body); !slices.Equal(resp.failed(), []string{guardUnhealthy}) || !strings.Contains(body, "node-37 as healthy") || len(fake.deleted) != 0 {
		t.Errorf("got %s", body)
	}
}

func TestHealthCheckErrorsAreReported(t *testing.T) {
	fake := newFake()
	fake.healthErr = core.Providerf("compute API: denied")

	status, _ := invoke(t, fake, request("plan"))

	if status != http.StatusBadGateway {
		t.Errorf("status = %d", status)
	}
}
