package sshaccess

import (
	"context"
	"encoding/json"
	"maps"
	"net/http"
	"net/http/httptest"
	"slices"
	"strings"
	"testing"
	"time"

	"cloud.google.com/go/compute/apiv1/computepb"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iap"
)

const (
	now    = "2026-10-02T08:00:00Z"
	member = "user:alice@example.com"
)

// fakeCloud is Compute Engine and IAP: it holds an instance, the project metadata and the two
// policies, and records the policies written.
type fakeCloud struct {
	// Disks, groups and load balancers aren't used by this function, nor instance changes;
	// calling them panics.
	compute.Disks
	compute.Instances
	compute.Groups
	compute.LoadBalancers

	instance *compute.Instance
	project  map[string]string
	login    *iam.Policy
	tunnel   *iam.Policy

	loginWrites, tunnelWrites int
	writes                    []string
	failTunnel                bool
}

func (f *fakeCloud) Connect(context.Context) (compute.Client, error) { return f, nil }

type iapConnector struct{ f *fakeCloud }

func (c iapConnector) Connect(context.Context) (iap.Client, error) { return c.f, nil }

func (f *fakeCloud) Instance(context.Context, string, string) (*compute.Instance, error) {
	return f.instance, nil
}

func (f *fakeCloud) ProjectMetadata(context.Context) (map[string]string, error) {
	return f.project, nil
}

func (f *fakeCloud) InstancePolicy(context.Context, string, string) (*iam.Policy, error) {
	return clone(f.login), nil
}

func (f *fakeCloud) SetInstancePolicy(_ context.Context, _, _ string, policy *iam.Policy) error {
	f.login = policy
	f.loginWrites++
	f.writes = append(f.writes, "login")
	return nil
}

func (f *fakeCloud) TunnelPolicy(context.Context, string, string) (*iam.Policy, error) {
	return clone(f.tunnel), nil
}

func (f *fakeCloud) SetTunnelPolicy(_ context.Context, _, _ string, policy *iam.Policy) error {
	if f.failTunnel {
		return core.Providerf("IAP API: denied")
	}
	f.writes = append(f.writes, "tunnel")
	f.tunnel = policy
	f.tunnelWrites++
	return nil
}

func clone(p *iam.Policy) *iam.Policy {
	c := &iam.Policy{Etag: p.Etag}
	for _, b := range p.Bindings {
		copied := *b
		copied.Members = slices.Clone(b.Members)
		c.Bindings = append(c.Bindings, &copied)
	}
	return c
}

func newFake() *fakeCloud {
	return &fakeCloud{
		instance: &compute.Instance{
			Name:            new("vm-1"),
			Status:          new("RUNNING"),
			ServiceAccounts: []*computepb.ServiceAccount{{Email: new("vm@p.iam.gserviceaccount.com")}},
		},
		project: map[string]string{osLoginKey: "TRUE"},
		login:   &iam.Policy{Etag: "a", Bindings: []*iam.Binding{{Role: "roles/compute.viewer", Members: []string{member}}}},
		tunnel:  &iam.Policy{Etag: "b"},
	}
}

type fixedClock int64

func (c fixedClock) Now() int64 { return int64(c) }

func at(t *testing.T, value string) time.Time {
	t.Helper()
	parsed, err := time.Parse(time.RFC3339, value)
	if err != nil {
		t.Fatal(err)
	}
	return parsed
}

func invoke(t *testing.T, fake *fakeCloud, body string) (int, string) {
	t.Helper()
	t.Setenv(gcp.ProjectVar, "p")
	rec := httptest.NewRecorder()
	fn := &Function{compute: fake, iap: iapConnector{fake}, clock: fixedClock(at(t, now).Unix()), maxMinutes: 240}
	core.NewServer(fn).ServeHTTP(rec, httptest.NewRequest(http.MethodPost, "/", strings.NewReader(body)))
	return rec.Code, rec.Body.String()
}

func request(action string, extra map[string]any) string {
	fields := map[string]any{"action": action, "zone": "us-central1-a", "instance": "vm-1", "user": "alice@example.com"}
	maps.Copy(fields, extra)
	body, _ := json.Marshal(fields)
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

// granted returns the roles granted to the member through this function, with their expiry.
func granted(p *iam.Policy) []string {
	var grants []string
	for _, b := range p.Bindings {
		if at, ours := expiry(b); ours && slices.Contains(b.Members, member) {
			grants = append(grants, b.Role+"@"+at.Format(time.RFC3339))
		}
	}
	return grants
}

func TestPlanDescribesTheGrant(t *testing.T) {
	fake := newFake()

	status, body := invoke(t, fake, request("plan", nil))

	want := `{"action":"plan","guards":[` +
		`{"detail":"instance vm-1","name":"exists","passed":true},` +
		`{"detail":"OS Login is enabled (project metadata enable-oslogin=TRUE)","name":"os-login","passed":true},` +
		`{"detail":"60 minutes; the installation allows at most 240","name":"duration-allowed","passed":true}],` +
		`"outcome":"planned","result":{"access":{"expiresAt":"2026-10-02T09:00:00Z","role":"user"},"changed":false,` +
		`"command":"gcloud compute ssh vm-1 --zone=us-central1-a --project=p --tunnel-through-iap",` +
		`"instance":{"name":"vm-1","serviceAccount":"vm@p.iam.gserviceaccount.com","state":"RUNNING"},"member":"user:alice@example.com"}}`
	if status != http.StatusOK || body != want {
		t.Errorf("got %d %s\nwant %s", status, body, want)
	}
	if fake.loginWrites+fake.tunnelWrites != 0 {
		t.Errorf("wrote policies on plan")
	}
}

func TestExecuteGrantsLoginAndTunnelUntilTheExpiry(t *testing.T) {
	fake := newFake()

	_, body := invoke(t, fake, request("execute", map[string]any{"role": "admin", "durationMinutes": 30}))

	if got, _ := outcome(t, body); got != "done" || !strings.Contains(body, `"changed":true`) {
		t.Errorf("got %s", body)
	}
	if got := granted(fake.login); !slices.Equal(got, []string{"roles/compute.osAdminLogin@2026-10-02T08:30:00Z"}) {
		t.Errorf("login grants %v", got)
	}
	if got := granted(fake.tunnel); !slices.Equal(got, []string{"roles/iap.tunnelResourceAccessor@2026-10-02T08:30:00Z"}) {
		t.Errorf("tunnel grants %v", got)
	}
	// The unrelated binding and the etag the change was based on are kept.
	if fake.login.Bindings[0].Role != "roles/compute.viewer" || fake.login.Etag != "a" || fake.tunnel.Etag != "b" {
		t.Errorf("login %+v, tunnel %+v", fake.login, fake.tunnel)
	}
	b := fake.login.Bindings[1]
	if b.Condition.Expression != `request.time < timestamp("2026-10-02T08:30:00Z")` || b.Condition.Title != marker {
		t.Errorf("condition = %+v", b.Condition)
	}
}

func TestGrantingAgainExtendsAndNeverShortens(t *testing.T) {
	fake := newFake()
	fake.login.Bindings = append(fake.login.Bindings, binding(member, roleOSLogin, at(t, "2026-10-02T10:00:00Z")))
	fake.tunnel.Bindings = append(fake.tunnel.Bindings, binding(member, roleTunnel, at(t, "2026-10-02T10:00:00Z")))

	_, body := invoke(t, fake, request("execute", map[string]any{"durationMinutes": 30}))

	if !strings.Contains(body, `"access":{"expiresAt":"2026-10-02T10:00:00Z","role":"user"},"changed":false`) ||
		fake.loginWrites+fake.tunnelWrites != 0 {
		t.Errorf("got %s, writes %d/%d", body, fake.loginWrites, fake.tunnelWrites)
	}

	_, body = invoke(t, fake, request("execute", map[string]any{"durationMinutes": 180}))

	if got := granted(fake.login); !strings.Contains(body, `"changed":true`) || !slices.Equal(got, []string{"roles/compute.osLogin@2026-10-02T11:00:00Z"}) {
		t.Errorf("got %s, login grants %v", body, got)
	}
}

func TestSwitchingRoleReplacesTheGrant(t *testing.T) {
	fake := newFake()
	fake.login.Bindings = append(fake.login.Bindings, binding(member, roleOSAdminLogin, at(t, "2026-10-02T08:30:00Z")))

	invoke(t, fake, request("execute", nil))

	if got := granted(fake.login); !slices.Equal(got, []string{"roles/compute.osLogin@2026-10-02T09:00:00Z"}) {
		t.Errorf("login grants %v", got)
	}
}

func TestRevokeRemovesOnlyTheUsersGrants(t *testing.T) {
	fake := newFake()
	other := "user:bob@example.com"
	shared := binding(member, roleOSLogin, at(t, "2026-10-02T09:00:00Z"))
	shared.Members = append(shared.Members, other)
	fake.login.Bindings = append(fake.login.Bindings, shared, &iam.Binding{Role: roleOSLogin, Members: []string{member}})
	fake.tunnel.Bindings = append(fake.tunnel.Bindings, binding(member, roleTunnel, at(t, "2026-10-02T09:00:00Z")))
	// OS Login is off, which doesn't matter for revoking.
	fake.project = map[string]string{}

	_, body := invoke(t, fake, request("execute", map[string]any{"revoke": true}))

	if got, _ := outcome(t, body); got != "done" || strings.Contains(body, `"access"`) || strings.Contains(body, `"command"`) ||
		!strings.Contains(body, `"otherAccess":["roles/compute.osLogin"]`) {
		t.Errorf("got %s", body)
	}
	if len(granted(fake.login))+len(granted(fake.tunnel)) != 0 {
		t.Errorf("grants left: %v %v", granted(fake.login), granted(fake.tunnel))
	}
	// Bob keeps his access and the grant not made by the function stays.
	if b := fake.login.Bindings[2]; b.Condition != nil || b.Role != roleOSLogin {
		t.Errorf("binding = %+v", b)
	}
	if b := fake.login.Bindings[1]; !slices.Equal(b.Members, []string{other}) || b.Condition.Title != marker {
		t.Errorf("binding = %+v", b)
	}
}

func TestExecuteRemovesExpiredGrants(t *testing.T) {
	fake := newFake()
	fake.login.Bindings = append(fake.login.Bindings, binding("user:bob@example.com", roleOSLogin, at(t, "2026-10-02T07:00:00Z")))

	_, body := invoke(t, fake, request("execute", map[string]any{"revoke": true}))

	if !strings.Contains(body, `"changed":true,"expiredRemoved":1`) || len(fake.login.Bindings) != 1 {
		t.Errorf("got %s, bindings %+v", body, fake.login.Bindings)
	}
}

func TestRefusesWithoutChangingAnything(t *testing.T) {
	for name, tc := range map[string]struct {
		change func(*fakeCloud)
		extra  map[string]any
		failed []string
	}{
		"no OS Login": {func(f *fakeCloud) { f.project = map[string]string{} }, nil, []string{guardOSLogin}},
		"disabled on the instance": {func(f *fakeCloud) {
			f.instance.Metadata = &computepb.Metadata{Items: []*computepb.Items{{Key: new(osLoginKey), Value: new("FALSE")}}}
		}, nil, []string{guardOSLogin}},
		"too long":         {func(*fakeCloud) {}, map[string]any{"durationMinutes": 300}, []string{guardDurationAllowed}},
		"missing instance": {func(f *fakeCloud) { f.instance = nil }, nil, []string{guardExists}},
	} {
		t.Run(name, func(t *testing.T) {
			fake := newFake()
			tc.change(fake)

			_, body := invoke(t, fake, request("execute", tc.extra))

			if got, failed := outcome(t, body); got != "refused" || !slices.Equal(failed, tc.failed) || fake.loginWrites+fake.tunnelWrites != 0 {
				t.Errorf("got %s, want refused with %v failed", body, tc.failed)
			}
		})
	}
}

func TestInvalidParamsAreRejected(t *testing.T) {
	for name, extra := range map[string]map[string]any{
		"zone":            {"zone": "us-central1"},
		"instance":        {"instance": "VM"},
		"user":            {"user": "serviceAccount:x@p.iam.gserviceaccount.com"},
		"role":            {"role": "root"},
		"durationMinutes": {"durationMinutes": 0},
	} {
		status, body := invoke(t, nil, request("plan", extra))

		if status != http.StatusBadRequest || !strings.Contains(body, name+" ") {
			t.Errorf("%s: got %d %s", name, status, body)
		}
	}
}

func TestMaxMinutesFromEnv(t *testing.T) {
	for value, want := range map[string]int64{"": maxMinutes, "60": 60, "5000": maxMinutes, "0": maxMinutes} {
		t.Setenv(MaxMinutesVar, value)

		if got := MaxMinutesFromEnv(); got != want {
			t.Errorf("%q: got %d, want %d", value, got, want)
		}
	}
}

// A failed write must not leave the user with login access but no way to revoke it, nor with
// login access after a revoke.
func TestLoginIsGrantedLastAndRevokedFirst(t *testing.T) {
	fake := newFake()
	invoke(t, fake, request("execute", nil))
	if !slices.Equal(fake.writes, []string{"tunnel", "login"}) {
		t.Errorf("grant wrote %v", fake.writes)
	}

	fake.writes = nil
	invoke(t, fake, request("execute", map[string]any{"revoke": true}))
	if !slices.Equal(fake.writes, []string{"login", "tunnel"}) {
		t.Errorf("revoke wrote %v", fake.writes)
	}
}

func TestFailedTunnelWriteGrantsNoLogin(t *testing.T) {
	fake := newFake()
	fake.failTunnel = true

	status, _ := invoke(t, fake, request("execute", nil))

	if status != http.StatusBadGateway || len(granted(fake.login)) != 0 {
		t.Errorf("got %d, login grants %v", status, granted(fake.login))
	}
}
