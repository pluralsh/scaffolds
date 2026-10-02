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

	c, err := newSDKClient(t.Context(), "test-project", srv.URL, option.WithoutAuthentication())
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

func TestInstanceReturnsTheInstance(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/instances/vm-1" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"vm-1","status":"RUNNING","deletionProtection":true,`+
			`"disks":[{"deviceName":"vm-1","boot":true,"autoDelete":false,"type":"PERSISTENT","source":"https://x/disks/vm-1"}],`+
			`"metadata":{"items":[{"key":"created-by","value":"projects/1/zones/z/instanceGroupManagers/web"}]}}`)
	})

	instance, err := c.Instance(t.Context(), "us-central1-a", "vm-1")

	if err != nil || instance == nil {
		t.Fatalf("got %v, %v", instance, err)
	}
	disk := instance.GetDisks()[0]
	if instance.GetName() != "vm-1" || instance.GetStatus() != "RUNNING" || !instance.GetDeletionProtection() ||
		disk.GetDeviceName() != "vm-1" || !disk.GetBoot() || disk.GetAutoDelete() || disk.GetSource() != "https://x/disks/vm-1" ||
		instance.GetMetadata().GetItems()[0].GetValue() != "projects/1/zones/z/instanceGroupManagers/web" {
		t.Errorf("instance = %v", instance)
	}
}

func TestInstanceIsNilWhenNotFound(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		writeJSON(w, http.StatusNotFound, `{"error":{"code":404,"message":"The resource was not found","errors":[{"reason":"notFound"}]}}`)
	})

	instance, err := c.Instance(t.Context(), "us-central1-a", "vm-1")

	if instance != nil || err != nil {
		t.Errorf("got %v, %v", instance, err)
	}
}

func TestSetDiskAutoDeleteStartsAnOperation(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		q := r.URL.Query()
		if r.Method != http.MethodPost || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/instances/vm-1/setDiskAutoDelete" ||
			q.Get("deviceName") != "vm-1-data" || q.Get("autoDelete") != "false" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"operation-3","status":"RUNNING"}`)
	})

	op, err := c.SetDiskAutoDelete(t.Context(), "us-central1-a", "vm-1", "vm-1-data", false)

	if err != nil || op != "operation-3" {
		t.Errorf("got %q, %v", op, err)
	}
}

func TestDeleteInstanceStartsAnOperation(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodDelete || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/instances/vm-1" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"operation-4","status":"RUNNING"}`)
	})

	op, err := c.DeleteInstance(t.Context(), "us-central1-a", "vm-1")

	if err != nil || op != "operation-4" {
		t.Errorf("got %q, %v", op, err)
	}
}

func TestInstanceGroupManagerReturnsTheGroup(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/compute/v1/projects/test-project/zones/us-central1-a/instanceGroupManagers/gke-c-pool-grp" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"gke-c-pool-grp","targetSize":3}`)
	})

	group, err := c.InstanceGroupManager(t.Context(), "us-central1-a", "gke-c-pool-grp")

	if err != nil || group.GetName() != "gke-c-pool-grp" || group.GetTargetSize() != 3 {
		t.Errorf("got %v, %v", group, err)
	}
}

func TestProjectMetadata(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet || r.URL.Path != "/compute/v1/projects/test-project" {
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
		writeJSON(w, http.StatusOK, `{"name":"test-project","commonInstanceMetadata":{"items":[{"key":"enable-oslogin","value":"TRUE"}]}}`)
	})

	metadata, err := c.ProjectMetadata(t.Context())

	if err != nil || metadata["enable-oslogin"] != "TRUE" {
		t.Errorf("got %v, %v", metadata, err)
	}
}

func TestInstancePolicyRoundTrip(t *testing.T) {
	c := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/compute/v1/projects/test-project/zones/us-central1-a/instances/vm-1/getIamPolicy":
			if r.URL.Query().Get("optionsRequestedPolicyVersion") != "3" {
				t.Errorf("policy version not requested: %s", r.URL)
			}
			writeJSON(w, http.StatusOK, `{"etag":"BwY=","version":3,"bindings":[{"role":"roles/compute.osLogin","members":["user:a@example.com"],`+
				`"condition":{"title":"t","description":"d","expression":"e"}}]}`)
		case "/compute/v1/projects/test-project/zones/us-central1-a/instances/vm-1/setIamPolicy":
			body, _ := io.ReadAll(r.Body)
			if !strings.Contains(string(body), `"etag":"BwY="`) || !strings.Contains(string(body), `"version":3`) || !strings.Contains(string(body), `"title":"t"`) {
				t.Errorf("body = %s", body)
			}
			writeJSON(w, http.StatusOK, `{}`)
		default:
			t.Errorf("unexpected request %s %s", r.Method, r.URL)
		}
	})

	policy, err := c.InstancePolicy(t.Context(), "us-central1-a", "vm-1")
	if err != nil || policy.Etag != "BwY=" || policy.Bindings[0].Condition.Description != "d" {
		t.Fatalf("got %+v, %v", policy, err)
	}
	if err := c.SetInstancePolicy(t.Context(), "us-central1-a", "vm-1", policy); err != nil {
		t.Error(err)
	}
}
