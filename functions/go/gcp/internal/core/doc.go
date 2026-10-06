// Package core holds the cloud-agnostic building blocks shared by all operational functions.
//
// A function implements [Handler] and is served over HTTP by a [Server]. It receives a
// [Request] (the tool input from a Plural workbench) and answers with a [Response]. Requests
// default to [ActionPlan], a dry run that only evaluates [Guard]s, so nothing changes unless
// the caller asks to execute.
package core
