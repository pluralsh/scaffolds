package iap

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
)

const tunnelPath = "/v1/projects/test-project/iap_tunnel/zones/us-central1-a/instances/vm-1"

func newTestClient(t *testing.T, handler http.HandlerFunc) *apiClient {
	t.Helper()
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	c, err := newAPIClient(t.Context(), "test-project", option.WithEndpoint(srv.URL+"/"), option.WithoutAuthentication())
	if err != nil {
		t.Fatal(err)
	}
	return c
}

func TestTunnelPolicyAsksForConditions(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		if r.Method != http.MethodPost || r.URL.Path != tunnelPath+":getIamPolicy" || !strings.Contains(string(body), `"requestedPolicyVersion":3`) {
			t.Errorf("unexpected request %s %s %s", r.Method, r.URL, body)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"etag":"BwY=","version":3,"bindings":[{"role":"roles/iap.tunnelResourceAccessor","members":["user:a@example.com"],`+
			`"condition":{"title":"t","expression":"e"}}]}`)
	})

	policy, err := c.TunnelPolicy(t.Context(), "us-central1-a", "vm-1")

	if err != nil || policy.Etag != "BwY=" || len(policy.Bindings) != 1 || policy.Bindings[0].Condition.Title != "t" ||
		policy.Bindings[0].Members[0] != "user:a@example.com" {
		t.Errorf("got %+v, %v", policy, err)
	}
}

func TestSetTunnelPolicySendsVersion3WithTheEtag(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		var body struct {
			Policy struct {
				Etag     string
				Version  int
				Bindings []struct {
					Role      string
					Condition struct{ Title string }
				}
			}
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		if r.URL.Path != tunnelPath+":setIamPolicy" || body.Policy.Etag != "BwY=" || body.Policy.Version != 3 ||
			body.Policy.Bindings[0].Condition.Title != "t" {
			t.Errorf("unexpected request %s %+v", r.URL, body)
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{}`)
	})

	err := c.SetTunnelPolicy(t.Context(), "us-central1-a", "vm-1", &iam.Policy{Etag: "BwY=", Bindings: []*iam.Binding{
		{Role: "roles/iap.tunnelResourceAccessor", Members: []string{"user:a@example.com"}, Condition: &iam.Condition{Title: "t", Expression: "e"}},
	}})

	if err != nil {
		t.Error(err)
	}
}
