package sshaccess

import (
	"fmt"
	"regexp"
	"slices"
	"time"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
)

const (
	// marker is the title of the conditions of the bindings this function creates. Only such
	// bindings are changed or removed. Anyone who may set the policy could create one, so it is
	// a label, not a proof.
	marker = "plural.sh ssh-access"

	// The roles granted: OS Login without or with sudo on the instance, and tunnelling to it
	// through IAP.
	roleOSLogin      = "roles/compute.osLogin"
	roleOSAdminLogin = "roles/compute.osAdminLogin"
	roleTunnel       = "roles/iap.tunnelResourceAccessor"

	// The IAM member prefixes of users and service accounts, and the email domain suffix of
	// service accounts.
	userPrefix           = "user:"
	serviceAccountPrefix = "serviceAccount:"
	serviceAccountDomain = ".gserviceaccount.com"
)

// expiryExpression is the condition of a binding that holds until the timestamp.
var expiryExpression = regexp.MustCompile(`^request\.time < timestamp\("([^"]+)"\)$`)

// loginRole is the OS Login role of the access role.
func loginRole(role Role) string {
	if role == RoleAdmin {
		return roleOSAdminLogin
	}
	return roleOSLogin
}

// accessRole is the access role of an OS Login role.
func accessRole(role string) Role {
	if role == roleOSAdminLogin {
		return RoleAdmin
	}
	return RoleUser
}

// expiry is when a binding created by this function expires, or false if it isn't one.
func expiry(b *iam.Binding) (time.Time, bool) {
	if b.Condition == nil || b.Condition.Title != marker {
		return time.Time{}, false
	}
	match := expiryExpression.FindStringSubmatch(b.Condition.Expression)
	if match == nil {
		return time.Time{}, false
	}
	at, err := time.Parse(time.RFC3339, match[1])
	return at, err == nil
}

// policy is an IAM policy whose bindings of some roles this function manages.
type policy struct {
	*iam.Policy
	// roles are the roles of the bindings this function manages in this policy.
	roles []string
}

// grants returns the roles the member has through this function in the policy, with when
// they expire, ignoring expired ones.
func (p policy) grants(member string, now time.Time) map[string]time.Time {
	grants := map[string]time.Time{}
	for _, b := range p.Bindings {
		at, ours := expiry(b)
		if ours && slices.Contains(p.roles, b.Role) && slices.Contains(b.Members, member) && at.After(now) && at.After(grants[b.Role]) {
			grants[b.Role] = at
		}
	}
	return grants
}

// others returns the managed roles the member has in the policy other than through this
// function.
func (p policy) others(member string) []string {
	var roles []string
	for _, b := range p.Bindings {
		if _, ours := expiry(b); !ours && slices.Contains(p.roles, b.Role) && slices.Contains(b.Members, member) && !slices.Contains(roles, b.Role) {
			roles = append(roles, b.Role)
		}
	}
	return roles
}

// edit removes the expired bindings of this function and the member's own, then grants role
// until expiresAt unless role is empty. Granting exactly what the member already has isn't a
// change.
// It reports whether the policy changed and how many expired bindings it removed.
func (p policy) edit(member, role string, expiresAt, now time.Time) (changed bool, expired int) {
	var others, own []*iam.Binding
	for _, b := range p.Bindings {
		at, ours := expiry(b)
		switch {
		case !ours || !slices.Contains(p.roles, b.Role):
			others = append(others, b)
		case !at.After(now):
			expired++
		case slices.Contains(b.Members, member):
			own = append(own, b)
			if members := slices.DeleteFunc(slices.Clone(b.Members), func(m string) bool { return m == member }); len(members) > 0 {
				others = append(others, &iam.Binding{Role: b.Role, Members: members, Condition: b.Condition})
			}
		default:
			others = append(others, b)
		}
	}

	// The member already has exactly the access to grant: the policy is only rebuilt.
	exact := role != "" && len(own) == 1 && own[0].Role == role && slices.Equal(own[0].Members, []string{member})
	if exact {
		at, _ := expiry(own[0])
		exact = at.Equal(expiresAt)
	}
	if role != "" {
		others = append(others, binding(member, role, expiresAt))
	}
	p.Bindings = others
	return expired > 0 || (!exact && (len(own) > 0 || role != "")), expired
}

// binding grants role to the member until expiresAt.
func binding(member, role string, expiresAt time.Time) *iam.Binding {
	at := expiresAt.UTC().Format(time.RFC3339)
	return &iam.Binding{
		Role:    role,
		Members: []string{member},
		Condition: &iam.Condition{
			Title:       marker,
			Description: "SSH access granted by the Plural ssh-access function until " + at,
			Expression:  fmt.Sprintf("request.time < timestamp(%q)", at),
		},
	}
}
