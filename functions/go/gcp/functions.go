// Package functions holds the operational functions deployed to Cloud Run (Cloud Functions
// 2nd gen), each registered under its own entry point.
//
// A function receives the tool input sent by a Plural workbench as the body of a POST and
// answers with the envelope of [core.Response], see [core.Server]. Each function lives in its
// own package under internal.
package functions

import (
	"log/slog"
	"os"

	"github.com/GoogleCloudPlatform/functions-framework-go/functions"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volumedelete"
)

func init() {
	slog.SetDefault(core.NewLogger(os.Stdout))

	functions.HTTP("VolumeDelete", core.NewServer(volumedelete.New()).ServeHTTP)
}
