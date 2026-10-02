package compute

import (
	"errors"
	"net/http"
	"slices"
	"testing"
)

const regionPath = "/compute/v1/projects/test-project/regions/us-central1"

// lbServer answers for a few load balancer resources named a1, and 404 for anything else.
func lbServer(w http.ResponseWriter, r *http.Request) {
	switch r.URL.Path {
	case regionPath + "/forwardingRules/a1":
		writeJSON(w, http.StatusOK, `{"name":"a1","description":"d","IPAddress":"1.2.3.4","target":"https://x/regions/us-central1/targetPools/a1"}`)
	case regionPath + "/backendServices/a1":
		writeJSON(w, http.StatusOK, `{"name":"a1","backends":[{"group":"g1"}],"healthChecks":["h1"]}`)
	case "/compute/v1/projects/test-project/global/httpHealthChecks/a1":
		writeJSON(w, http.StatusOK, `{"name":"a1","description":"legacy","selfLink":"s"}`)
	case regionPath + "/addresses/a1":
		writeJSON(w, http.StatusOK, `{"name":"a1","address":"1.2.3.4","status":"RESERVED"}`)
	default:
		writeJSON(w, http.StatusNotFound, `{"error":{"code":404,"message":"not found"}}`)
	}
}

func TestLBResourceReadsForwardingRules(t *testing.T) {
	rule, err := newTestClient(t, lbServer).LBResource(t.Context(), KindForwardingRule, "us-central1", "a1")

	if err != nil || rule.Kind != KindForwardingRule || rule.Description != "d" || rule.IPAddress != "1.2.3.4" ||
		rule.Target != "https://x/regions/us-central1/targetPools/a1" {
		t.Errorf("rule = %+v, %v", rule, err)
	}
}

func TestLBResourceReadsBackendServices(t *testing.T) {
	service, err := newTestClient(t, lbServer).LBResource(t.Context(), KindBackendService, "us-central1", "a1")

	if err != nil || !slices.Equal(service.Members, []string{"g1"}) || !slices.Equal(service.HealthChecks, []string{"h1"}) {
		t.Errorf("service = %+v, %v", service, err)
	}
}

func TestLBResourceReadsLegacyHealthChecks(t *testing.T) {
	check, err := newTestClient(t, lbServer).LBResource(t.Context(), KindHTTPHealthCheck, "", "a1")

	if err != nil || check.Kind != KindHTTPHealthCheck || check.Description != "legacy" {
		t.Errorf("check = %+v, %v", check, err)
	}
}

func TestLBResourceReadsAddresses(t *testing.T) {
	address, err := newTestClient(t, lbServer).LBResource(t.Context(), KindAddress, "us-central1", "a1")

	if err != nil || address.Status != "RESERVED" || address.IPAddress != "1.2.3.4" {
		t.Errorf("address = %+v, %v", address, err)
	}
}

func TestLBResourceIsNilWhenNotFound(t *testing.T) {
	c := newTestClient(t, lbServer)
	for _, kind := range []LBKind{KindTargetPool, KindHealthCheck, KindFirewall, KindHTTPHealthCheck} {
		if missing, err := c.LBResource(t.Context(), kind, "us-central1", "b1"); missing != nil || err != nil {
			t.Errorf("%s: got %+v, %v", kind, missing, err)
		}
	}
}

func TestForwardingRulesTakesTargetOrBackendService(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != regionPath+"/forwardingRules" {
			t.Errorf("unexpected path %s", r.URL.Path)
		}
		writeJSON(w, http.StatusOK, `{"items":[{"name":"a","target":"t"},{"name":"b","backendService":"s"}]}`)
	})

	rules, err := c.ForwardingRules(t.Context(), "us-central1")

	if err != nil || len(rules) != 2 || rules[0].Target != "t" || rules[1].Target != "s" {
		t.Errorf("got %+v, %v", rules, err)
	}
}

func TestHealthyAsksTheTargetAboutTheMember(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case regionPath + "/targetPools/a1/getHealth":
			writeJSON(w, http.StatusOK, `{"healthStatus":[{"healthState":"UNHEALTHY"}]}`)
		case regionPath + "/backendServices/a1/getHealth":
			writeJSON(w, http.StatusOK, `{"healthStatus":[{"healthState":"UNHEALTHY"},{"healthState":"HEALTHY"}]}`)
		default:
			t.Errorf("unexpected request %s", r.URL)
		}
	})
	selfLink := "https://www.googleapis.com/compute/v1/projects/test-project/regions/us-central1/"

	pool, err := c.Healthy(t.Context(), &LBResource{Kind: KindTargetPool, Name: "a1", SelfLink: selfLink + "targetPools/a1"}, "i1")
	if err != nil || pool {
		t.Errorf("pool: %v, %v", pool, err)
	}
	service, err := c.Healthy(t.Context(), &LBResource{Kind: KindBackendService, Name: "a1", SelfLink: selfLink + "backendServices/a1"}, "g1")
	if err != nil || !service {
		t.Errorf("service: %v, %v", service, err)
	}
}

func TestDeleteLBResourceReportsBusyResources(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case regionPath + "/targetPools/a1":
			writeJSON(w, http.StatusBadRequest, `{"error":{"code":400,"message":"in use","errors":[{"reason":"resourceInUseByAnotherResource"}]}}`)
		case "/compute/v1/projects/test-project/global/httpHealthChecks/a1":
			writeJSON(w, http.StatusOK, `{"name":"operation-legacy"}`)
		case "/compute/v1/projects/test-project/global/firewalls/k8s-fw-a1":
			writeJSON(w, http.StatusForbidden, `{"error":{"code":403,"message":"denied"}}`)
		default:
			writeJSON(w, http.StatusOK, `{"name":"operation-1"}`)
		}
	})

	if _, err := c.DeleteLBResource(t.Context(), KindTargetPool, "us-central1", "a1"); !errors.Is(err, ErrBusy) {
		t.Errorf("target pool: %v", err)
	}
	if _, err := c.DeleteLBResource(t.Context(), KindFirewall, "", "k8s-fw-a1"); err == nil || errors.Is(err, ErrBusy) {
		t.Errorf("firewall: %v", err)
	}
	if op, err := c.DeleteLBResource(t.Context(), KindHTTPHealthCheck, "", "a1"); err != nil || op != "operation-legacy" {
		t.Errorf("health check: %q, %v", op, err)
	}
	if op, err := c.DeleteLBResource(t.Context(), KindAddress, "us-central1", "a1"); err != nil || op != "operation-1" {
		t.Errorf("address: %q, %v", op, err)
	}
}

func TestHealthyOfAMemberThatIsGone(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusBadRequest, `{"error":{"code":400,"message":"The resource 'node-1' was not found"}}`)
	})

	healthy, err := c.Healthy(t.Context(), &LBResource{Kind: KindTargetPool, Name: "a1",
		SelfLink: "https://www.googleapis.com/compute/v1/projects/test-project/regions/us-central1/targetPools/a1"}, "node-1")

	if healthy || err != nil {
		t.Errorf("got %v, %v", healthy, err)
	}
}
