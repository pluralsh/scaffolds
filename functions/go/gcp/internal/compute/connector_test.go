package compute

import (
	"testing"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/gcp"
)

func TestConnectNeedsAProject(t *testing.T) {
	for _, value := range []string{"", "  "} {
		t.Setenv(gcp.ProjectVar, value)

		_, err := NewEnvConnector().Connect(t.Context())

		if want := "cloud provider error: GOOGLE_CLOUD_PROJECT is not set"; err == nil || err.Error() != want {
			t.Errorf("%q: err = %v, want %q", value, err, want)
		}
	}
}
