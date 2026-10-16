package cloudsql

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

func TestInstanceReturnsTheInstance(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/v1/projects/test-project/instances/db" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"db","state":"RUNNABLE","databaseVersion":"POSTGRES_17",`+
			`"settings":{"backupConfiguration":{"pointInTimeRecoveryEnabled":true,"transactionLogRetentionDays":7}}}`)
	})

	instance, err := c.Instance(t.Context(), "db")

	if err != nil || instance == nil {
		t.Fatalf("got %v, %v", instance, err)
	}
	if instance.State != "RUNNABLE" || !instance.Settings.BackupConfiguration.PointInTimeRecoveryEnabled ||
		instance.Settings.BackupConfiguration.TransactionLogRetentionDays != 7 {
		t.Errorf("instance = %+v", instance)
	}
}

func TestInstanceIsNilWhenNotFound(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusNotFound, `{"error":{"code":404,"message":"The Cloud SQL instance does not exist."}}`)
	})

	instance, err := c.Instance(t.Context(), "db")

	if instance != nil || err != nil {
		t.Errorf("got %v, %v", instance, err)
	}
}

func TestInstanceReportsOtherFailuresAsProviderErrors(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusForbidden, `{"error":{"code":403,"message":"not authorized"}}`)
	})

	_, err := c.Instance(t.Context(), "db")

	var e *core.Error
	if !errors.As(err, &e) || e.Kind != core.KindProvider || !strings.Contains(e.Message, "Cloud SQL Admin API: ") {
		t.Errorf("err = %v", err)
	}
}

func TestRecoveryWindow(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/v1/projects/test-project/instances/db/getLatestRecoveryTime" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"earliestRecoveryTime":"2026-09-25T08:00:00Z","latestRecoveryTime":"2026-10-02T07:59:00Z"}`)
	})

	window, err := c.RecoveryWindow(t.Context(), "db")

	if err != nil || window != (RecoveryWindow{Earliest: "2026-09-25T08:00:00Z", Latest: "2026-10-02T07:59:00Z"}) {
		t.Errorf("got %+v, %v", window, err)
	}
}

func TestCloneStartsAnOperation(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/v1/projects/test-project/instances/db/clone" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		var body struct {
			CloneContext struct{ DestinationInstanceName, PointInTime string }
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		if body.CloneContext.DestinationInstanceName != "db-restored" || body.CloneContext.PointInTime != "2026-10-01T08:00:00Z" {
			t.Errorf("body = %+v", body)
		}
		writeJSON(w, http.StatusOK, `{"name":"operation-1","status":"PENDING"}`)
	})

	op, err := c.Clone(t.Context(), "db", "db-restored", "2026-10-01T08:00:00Z")

	if err != nil || op != "operation-1" {
		t.Errorf("got %q, %v", op, err)
	}
}

func TestInstanceName(t *testing.T) {
	for name, valid := range map[string]bool{"db": true, "db-restored-1": true, "": false, "1db": false, "db-": false, "DB": false, "db_1": false, strings.Repeat("a", 99): false} {
		if InstanceName(name).Valid() != valid {
			t.Errorf("InstanceName(%q).Valid() != %v", name, valid)
		}
	}
}
