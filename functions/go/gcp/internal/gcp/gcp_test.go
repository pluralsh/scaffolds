package gcp

import (
	"context"
	"errors"
	"testing"
)

func TestLazyCreatesOnceAndRetriesFailures(t *testing.T) {
	t.Setenv(ProjectVar, "p")
	calls := 0
	lazy := NewLazy(func(_ context.Context, project string) (string, error) {
		calls++
		if calls == 1 {
			return "", errors.New("boom")
		}
		return "client for " + project, nil
	})

	if _, err := lazy.Get(t.Context()); err == nil {
		t.Fatal("first call should fail")
	}
	for range 2 {
		if client, err := lazy.Get(t.Context()); err != nil || client != "client for p" {
			t.Errorf("got %q, %v", client, err)
		}
	}
	if calls != 2 {
		t.Errorf("created %d times", calls)
	}
}

func TestProjectMustBeSet(t *testing.T) {
	t.Setenv(ProjectVar, " ")

	if _, err := Project(); err == nil || err.Error() != "cloud provider error: GOOGLE_CLOUD_PROJECT is not set" {
		t.Errorf("err = %v", err)
	}
}
