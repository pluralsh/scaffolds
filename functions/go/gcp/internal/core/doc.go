// Package core holds the cloud-agnostic building blocks shared by every operational function.
//
// Each function receives a [Request] (the tool input sent by a Plural workbench) and answers
// with a [Response]. Requests default to [ActionPlan], a dry run that only evaluates
// [Guard]s, so nothing is changed unless the caller explicitly asks to execute.
//
// A function implements [Handler] and is served over HTTP by a [Server].
package core
