package sshaccess

import (
	"os"
	"strconv"
	"strings"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
)

const (
	// MaxMinutesVar is the environment variable with the longest access callers may grant, at
	// most [maxMinutes].
	MaxMinutesVar = "MAX_DURATION_MINUTES"

	// maxMinutes is the longest access the function grants: a day.
	maxMinutes = 1440
	// defaultMinutes is how long the access lasts unless the caller says otherwise.
	defaultMinutes = 60
)

// MaxMinutesFromEnv is the limit set by [MaxMinutesVar], or [maxMinutes] if it is unset or
// invalid.
func MaxMinutesFromEnv() int64 {
	limit, err := strconv.ParseInt(os.Getenv(MaxMinutesVar), 10, 64)
	if err != nil || limit < 1 {
		return maxMinutes
	}
	return min(limit, maxMinutes)
}

// Role is the OS Login access the principal gets.
type Role string

const (
	// RoleUser logs in without sudo.
	RoleUser Role = "user"
	// RoleAdmin logs in with sudo.
	RoleAdmin Role = "admin"
)

// Params are the operation-specific fields of the tool input.
type Params struct {
	Zone     string `json:"zone" required:"true"`
	Instance string `json:"instance" required:"true"`
	// Principal is the email of the user or service account that gets the access.
	Principal string `json:"principal" required:"true"`
	// Role defaults to [RoleUser].
	Role Role `json:"role"`
	// DurationMinutes defaults to [defaultMinutes].
	DurationMinutes *int64 `json:"durationMinutes"`
	// Revoke removes the access instead of granting it.
	Revoke bool `json:"revoke"`
}

// Validate checks the parameters before anything is asked of GCP.
func (p Params) Validate() error {
	if !compute.Zone(p.Zone).Valid() {
		return core.InvalidRequestf("zone %q is not a Compute Engine zone (e.g. us-central1-a)", p.Zone)
	}
	if !compute.ResourceName(p.Instance).Valid() {
		return core.InvalidRequestf("instance %q is not a Compute Engine instance name", p.Instance)
	}
	if !validEmail(p.Principal) {
		return core.InvalidRequestf("principal %q is not the email of a user or service account", p.Principal)
	}
	if p.Role != "" && p.Role != RoleUser && p.Role != RoleAdmin {
		return core.InvalidRequestf("role %q is not %s or %s", p.Role, RoleUser, RoleAdmin)
	}
	if p.DurationMinutes != nil && (*p.DurationMinutes < 1 || *p.DurationMinutes > maxMinutes) {
		return core.InvalidRequestf("durationMinutes %d is not between 1 and %d", *p.DurationMinutes, maxMinutes)
	}
	return nil
}

// withDefaults fills in the optional fields the caller left out.
func (p Params) withDefaults() Params {
	if p.Role == "" {
		p.Role = RoleUser
	}
	if p.DurationMinutes == nil {
		p.DurationMinutes = new(int64(defaultMinutes))
	}
	return p
}

// member is the IAM member of the principal: a service account, if the email is in a service
// account domain, or a user.
func (p Params) member() string {
	_, domain, _ := strings.Cut(p.Principal, "@")
	if strings.HasSuffix(domain, serviceAccountDomain) {
		return serviceAccountPrefix + p.Principal
	}
	return userPrefix + p.Principal
}

// validEmail reports whether s looks like an email: a local part and a domain with a dot,
// without spaces or the characters IAM member strings use as separators.
func validEmail(s string) bool {
	local, domain, ok := strings.Cut(s, "@")
	return ok && local != "" && strings.Contains(domain, ".") && !strings.HasPrefix(domain, ".") &&
		!strings.HasSuffix(domain, ".") && !strings.ContainsAny(s, " \t\n:,\"") && !strings.Contains(domain, "@")
}

// Output is the result reported to the caller.
type Output struct {
	Instance *Summary `json:"instance,omitempty"`
	// Member is the IAM member the access is for, user:<email> or serviceAccount:<email>.
	Member string `json:"member"`
	// Access is the principal's access through this function after the call, if any.
	Access *Access `json:"access,omitempty"`
	// OtherAccess lists the OS Login roles the principal has on the instance other than through this
	// function. Roles granted on the project or above aren't listed.
	OtherAccess []string `json:"otherAccess,omitempty"`
	// Changed is whether the IAM policies were changed.
	Changed bool `json:"changed"`
	// ExpiredRemoved counts the expired bindings of this function removed from the policies.
	ExpiredRemoved int `json:"expiredRemoved,omitempty"`
	// Command connects to the instance, once access is granted.
	Command string `json:"command,omitempty"`
}

// Summary describes the instance.
type Summary struct {
	Name  string `json:"name"`
	State string `json:"state"`
	// ServiceAccount is the account the instance runs as. OS Login also requires the principal
	// to have roles/iam.serviceAccountUser on it, which this function doesn't grant.
	ServiceAccount string `json:"serviceAccount,omitempty"`
}

// Access is the access this function granted.
type Access struct {
	Role      Role   `json:"role"`
	ExpiresAt string `json:"expiresAt"`
}
