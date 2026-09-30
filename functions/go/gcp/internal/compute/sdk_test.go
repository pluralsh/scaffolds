package compute

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

// newTestClient returns a client talking to handler in place of Compute Engine.
func newTestClient(t *testing.T, handler http.HandlerFunc) *sdkClient {
	t.Helper()
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)

	c, err := newSDKClient(t.Context(), "test-project", option.WithEndpoint(srv.URL), option.WithoutAuthentication())
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

func TestDiskReturnsTheDisk(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/disks/pvc-1" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"id":"4242","name":"pvc-1","status":"READY","sizeGb":"10","users":["u1"],"description":"d"}`)
	})

	disk, err := c.Disk(t.Context(), "us-central1-a", "pvc-1")

	if err != nil || disk == nil {
		t.Fatalf("got %v, %v", disk, err)
	}
	if disk.GetId() != 4242 || disk.GetName() != "pvc-1" || disk.GetStatus() != "READY" || disk.GetSizeGb() != 10 ||
		len(disk.GetUsers()) != 1 || disk.GetDescription() != "d" {
		t.Errorf("disk = %v", disk)
	}
}

func TestDiskIsNilWhenNotFound(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusNotFound, `{"error":{"code":404,"message":"The resource was not found","errors":[{"reason":"notFound"}]}}`)
	})

	disk, err := c.Disk(t.Context(), "us-central1-a", "pvc-1")

	if err != nil || disk != nil {
		t.Errorf("got %v, %v", disk, err)
	}
}

func TestDiskReportsOtherFailuresAsProviderErrors(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusForbidden, `{"error":{"code":403,"message":"Required permission denied"}}`)
	})

	_, err := c.Disk(t.Context(), "us-central1-a", "pvc-1")

	var e *core.Error
	if !errors.As(err, &e) || e.Kind != core.KindProvider || !strings.Contains(e.Message, "compute API: ") || !strings.Contains(e.Message, "Required permission denied") {
		t.Errorf("err = %v", err)
	}
}

func TestSnapshotsFiltersByLabelAndFollowsPages(t *testing.T) {
	var pages []string
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/compute/v1/projects/test-project/global/snapshots" {
			t.Errorf("unexpected path %s", r.URL.Path)
		}
		if got, want := r.URL.Query().Get("filter"), `labels.plural-sh-volume-delete="4242"`; got != want {
			t.Errorf("filter = %q, want %q", got, want)
		}
		token := r.URL.Query().Get("pageToken")
		pages = append(pages, token)
		if token == "" {
			writeJSON(w, http.StatusOK, `{"items":[{"name":"a"},{"name":"b"}],"nextPageToken":"next"}`)
			return
		}
		writeJSON(w, http.StatusOK, `{"items":[{"name":"c"}]}`)
	})

	snapshots, err := c.Snapshots(t.Context(), "plural-sh-volume-delete", "4242")

	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, s := range snapshots {
		names = append(names, s.GetName())
	}
	if strings.Join(names, ",") != "a,b,c" || len(pages) != 2 || pages[1] != "next" {
		t.Errorf("names = %v, pages = %q", names, pages)
	}
}

func TestSnapshotsReportsFailures(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusInternalServerError, `{"error":{"code":500,"message":"backend error"}}`)
	})

	if _, err := c.Snapshots(t.Context(), "l", "v"); err == nil || !strings.Contains(err.Error(), "backend error") {
		t.Errorf("err = %v", err)
	}
}

func TestCreateSnapshotStartsAnOperation(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/disks/pvc-1/createSnapshot" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		var body struct {
			Name, Description string
			Labels            map[string]string
		}
		if err := json.NewDecoder(r.Body).Decode(&body); err != nil {
			t.Error(err)
		}
		if body.Name != "snap" || body.Description != "why" || body.Labels["l"] != "4242" {
			t.Errorf("body = %+v", body)
		}
		writeJSON(w, http.StatusOK, `{"name":"operation-1","status":"RUNNING"}`)
	})

	op, err := c.CreateSnapshot(t.Context(), SnapshotRequest{
		Zone:        "us-central1-a",
		Disk:        "pvc-1",
		Name:        "snap",
		Description: "why",
		Labels:      map[string]string{"l": "4242"},
	})

	if err != nil || op != "operation-1" {
		t.Errorf("got %q, %v", op, err)
	}
}

func TestDeleteDiskStartsAnOperation(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/disks/pvc-1" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"operation-2","status":"RUNNING"}`)
	})

	op, err := c.DeleteDisk(t.Context(), "us-central1-a", "pvc-1")

	if err != nil || op != "operation-2" {
		t.Errorf("got %q, %v", op, err)
	}
}

func TestOperationsNeedAName(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusOK, `{"status":"RUNNING"}`)
	})

	_, err := c.DeleteDisk(t.Context(), "us-central1-a", "pvc-1")

	if err == nil || err.Error() != "cloud provider error: compute API returned an operation without a name" {
		t.Errorf("err = %v", err)
	}
}
