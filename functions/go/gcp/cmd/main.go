// Command main serves the functions locally with the Functions Framework, e.g.
//
//	FUNCTION_TARGET=VolumeDelete GOOGLE_CLOUD_PROJECT=my-project go run ./cmd
//
// Cloud Run builds the functions from the module root instead and never runs this command.
package main

import (
	"log/slog"
	"os"

	"github.com/GoogleCloudPlatform/functions-framework-go/funcframework"

	// Registers the functions.
	_ "github.com/pluralsh/scaffolds/functions/go/gcp"
)

const defaultPort = "8080"

func main() {
	port := os.Getenv("PORT")
	if port == "" {
		port = defaultPort
	}
	slog.Info("listening", "port", port)
	if err := funcframework.Start(port); err != nil {
		slog.Error("serving failed", "error", err.Error())
		os.Exit(1)
	}
}
