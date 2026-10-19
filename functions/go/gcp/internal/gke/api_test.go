package gke

import (
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const clusterPath = "/v1/projects/test-project/locations/us-central1/clusters/c"

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

func writeJSON(w http.ResponseWriter, status int, body string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_, _ = io.WriteString(w, body)
}

func TestClusterReturnsTheClusterWithItsPools(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != clusterPath {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"c","status":"RUNNING","nodePools":[{"name":"apps","status":"RUNNING",`+
			`"autoscaling":{"enabled":true,"minNodeCount":1,"maxNodeCount":3},"instanceGroupUrls":["u"]}]}`)
	})

	cluster, err := c.Cluster(t.Context(), "us-central1", "c")

	if err != nil || cluster == nil {
		t.Fatalf("got %v, %v", cluster, err)
	}
	pool := cluster.NodePools[0]
	if cluster.Status != "RUNNING" || pool.Name != "apps" || !pool.Autoscaling.Enabled || pool.Autoscaling.MaxNodeCount != 3 ||
		len(pool.InstanceGroupUrls) != 1 {
		t.Errorf("cluster = %+v, pool = %+v", cluster, pool)
	}
}

func TestClusterIsNilWhenNotFound(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusNotFound, `{"error":{"code":404,"message":"Not found"}}`)
	})

	cluster, err := c.Cluster(t.Context(), "us-central1", "c")

	if cluster != nil || err != nil {
		t.Errorf("got %v, %v", cluster, err)
	}
}

func TestClusterReportsOtherFailuresAsProviderErrors(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusForbidden, `{"error":{"code":403,"message":"Permission denied"}}`)
	})

	_, err := c.Cluster(t.Context(), "us-central1", "c")

	var e *core.Error
	if !errors.As(err, &e) || e.Kind != core.KindProvider || !strings.Contains(e.Message, "GKE API: ") {
		t.Errorf("err = %v", err)
	}
}

func TestSetNodePoolSizeStartsAnOperation(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != clusterPath+"/nodePools/apps:setSize" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		var body map[string]any
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		// Zero must be sent, not omitted.
		if body["nodeCount"] != "0" && body["nodeCount"] != float64(0) {
			t.Errorf("body = %v", body)
		}
		writeJSON(w, http.StatusOK, `{"name":"operation-1","status":"RUNNING"}`)
	})

	op, err := c.SetNodePoolSize(t.Context(), "us-central1", "c", "apps", 0)

	if err != nil || op != "operation-1" {
		t.Errorf("got %q, %v", op, err)
	}
}

func TestNames(t *testing.T) {
	for location, valid := range map[string]bool{"us-central1": true, "us-central1-a": true, "us": false, "US-central1": false} {
		if Location(location).Valid() != valid {
			t.Errorf("Location(%q).Valid() != %v", location, valid)
		}
	}
	for name, valid := range map[string]bool{"apps": true, "pool-1": true, strings.Repeat("a", 41): false, "Pool": false, "1pool": false} {
		if Name(name).Valid() != valid {
			t.Errorf("Name(%q).Valid() != %v", name, valid)
		}
	}
}
