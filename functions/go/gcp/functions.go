// Package functions holds the operational functions deployed to Cloud Run (Cloud Functions
// 2nd gen). Each has its own entry point and its own package under internal.
//
// A function receives the tool input from a Plural workbench as a POST body and answers with
// a [core.Response], see [core.Server].
package functions

import (
	"log/slog"
	"os"

	"github.com/GoogleCloudPlatform/functions-framework-go/functions"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/dbrestore"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/lbfrontenddelete"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/nodepoolresize"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/sshaccess"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/vmdelete"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/volumedelete"
)

func init() {
	slog.SetDefault(core.NewLogger(os.Stdout))

	functions.HTTP("VolumeDelete", core.NewServer(volumedelete.New()).ServeHTTP)
	functions.HTTP("VMDelete", core.NewServer(vmdelete.New()).ServeHTTP)
	functions.HTTP("NodePoolResize", core.NewServer(nodepoolresize.New()).ServeHTTP)
	functions.HTTP("LBFrontendDelete", core.NewServer(lbfrontenddelete.New()).ServeHTTP)
	functions.HTTP("DBRestore", core.NewServer(dbrestore.New()).ServeHTTP)
	functions.HTTP("SSHAccess", core.NewServer(sshaccess.New()).ServeHTTP)
}
