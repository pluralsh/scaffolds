package sshaccess

import (
	"context"
	"fmt"
	"log/slog"
	"strings"
	"time"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iap"
)

// The names of the guards, in the order they are evaluated.
const (
	guardExists          = "exists"
	guardOSLogin         = "os-login"
	guardDurationAllowed = "duration-allowed"
)

// osLoginKey is the instance or project metadata key that enables OS Login when TRUE. The
// instance's value wins over the project's.
const osLoginKey = "enable-oslogin"

// instances are the Compute Engine calls of the function.
type instances interface {
	compute.Instances
	compute.Access
}

// operation is a single invocation of the function, after its input was validated.
type operation struct {
	instances  instances
	tunnels    iap.Client
	project    string
	now        int64
	action     core.Action
	params     Params
	maxMinutes int64
}

// run inspects the instance and its policies, evaluates the guards and changes the access if
// they allow.
func (o *operation) run(ctx context.Context) (core.Response[Output], error) {
	member := memberPrefix + o.params.User
	found, err := o.instances.Instance(ctx, o.params.Zone, o.params.Instance)
	if err != nil {
		return core.Response[Output]{}, err
	}
	if found == nil {
		return o.refuse(core.Guards{core.Fail(guardExists, "instance not found")}, Output{Member: member}), nil
	}

	guards := core.Guards{core.Pass(guardExists, "instance "+found.GetName())}
	if !o.params.Revoke {
		osLogin, err := o.osLoginGuard(ctx, found)
		if err != nil {
			return core.Response[Output]{}, err
		}
		guards = append(guards, osLogin, core.Check(guardDurationAllowed, *o.params.DurationMinutes <= o.maxMinutes,
			fmt.Sprintf("%d minutes; the installation allows at most %d", *o.params.DurationMinutes, o.maxMinutes)))
	}

	login, tunnel, err := o.policies(ctx)
	if err != nil {
		return core.Response[Output]{}, err
	}
	now := time.Unix(o.now, 0)
	role, expiresAt := o.target(login, tunnel, member, now)
	output := Output{
		Instance:    summary(found),
		Member:      member,
		OtherAccess: append(login.others(member), tunnel.others(member)...),
	}
	if role != "" {
		output.Access = &Access{Role: accessRole(role), ExpiresAt: expiresAt.UTC().Format(time.RFC3339)}
		output.Command = o.command()
	}
	if o.action == core.ActionPlan || !guards.Passed() {
		return o.refuse(guards, output), nil
	}

	if output.Changed, output.ExpiredRemoved, err = o.apply(ctx, login, tunnel, member, role, expiresAt, now); err != nil {
		return core.Response[Output]{}, err
	}
	slog.InfoContext(ctx, "updated SSH access", "zone", o.params.Zone, "instance", o.params.Instance, "member", member,
		"role", role, "access", output.Access, "changed", output.Changed, "expired_removed", output.ExpiredRemoved)
	return core.Done(guards, output), nil
}

// apply grants role until expiresAt to the member in both policies, or revokes the access when
// role is empty, removes expired grants, and writes the policies that changed. It reports
// whether any did and how many expired grants it removed.
func (o *operation) apply(ctx context.Context, login, tunnel policy, member, role string, expiresAt, now time.Time) (bool, int, error) {
	tunnelRole := ""
	if role != "" {
		tunnelRole = roleTunnel
	}
	loginChanged, loginExpired := login.edit(member, role, expiresAt, now)
	tunnelChanged, tunnelExpired := tunnel.edit(member, tunnelRole, expiresAt, now)
	writeLogin := func() error {
		if !loginChanged {
			return nil
		}
		return o.instances.SetInstancePolicy(ctx, o.params.Zone, o.params.Instance, login.Policy)
	}
	writeTunnel := func() error {
		if !tunnelChanged {
			return nil
		}
		return o.tunnels.SetTunnelPolicy(ctx, o.params.Zone, o.params.Instance, tunnel.Policy)
	}
	// The OS Login role is what lets the user in: a grant writes it last and a revoke first, so
	// if the other write fails, the user is left without access rather than with it.
	writes := []func() error{writeTunnel, writeLogin}
	if role == "" {
		writes = []func() error{writeLogin, writeTunnel}
	}
	for _, write := range writes {
		if err := write(); err != nil {
			return false, 0, err
		}
	}
	return loginChanged || tunnelChanged, loginExpired + tunnelExpired, nil
}

// refuse answers a plan, or an execute that changes nothing.
func (o *operation) refuse(guards core.Guards, output Output) core.Response[Output] {
	if o.action == core.ActionPlan {
		return core.Planned(guards, output)
	}
	return core.Refused[Output](guards).WithResult(output)
}

// osLoginGuard checks that the instance uses OS Login, so IAM roles control who logs in.
func (o *operation) osLoginGuard(ctx context.Context, instance *compute.Instance) (core.Guard, error) {
	for _, item := range instance.GetMetadata().GetItems() {
		if item.GetKey() == osLoginKey {
			return osLoginCheck(item.GetValue(), "instance"), nil
		}
	}
	metadata, err := o.instances.ProjectMetadata(ctx)
	if err != nil {
		return core.Guard{}, err
	}
	return osLoginCheck(metadata[osLoginKey], "project"), nil
}

func osLoginCheck(value, source string) core.Guard {
	if strings.EqualFold(value, "true") {
		return core.Pass(guardOSLogin, fmt.Sprintf("OS Login is enabled (%s metadata %s=%s)", source, osLoginKey, value))
	}
	return core.Fail(guardOSLogin, fmt.Sprintf("OS Login is not enabled (%s metadata %s=%q); set it to TRUE on the instance or project", source, osLoginKey, value))
}

// policies reads the IAM policies of the instance and of the IAP tunnel to it.
func (o *operation) policies(ctx context.Context) (login, tunnel policy, err error) {
	instancePolicy, err := o.instances.InstancePolicy(ctx, o.params.Zone, o.params.Instance)
	if err != nil {
		return policy{}, policy{}, err
	}
	tunnelPolicy, err := o.tunnels.TunnelPolicy(ctx, o.params.Zone, o.params.Instance)
	if err != nil {
		return policy{}, policy{}, err
	}
	return policy{instancePolicy, []string{roleOSLogin, roleOSAdminLogin}}, policy{tunnelPolicy, []string{roleTunnel}}, nil
}

// target is the OS Login role the member has through this function after an execute, and
// until when, or an empty role when revoking. Granting again never shortens the access.
func (o *operation) target(login, tunnel policy, member string, now time.Time) (string, time.Time) {
	if o.params.Revoke {
		return "", time.Time{}
	}
	expiresAt := now.Add(time.Duration(*o.params.DurationMinutes) * time.Minute).Truncate(time.Second)
	for _, grants := range []map[string]time.Time{login.grants(member, now), tunnel.grants(member, now)} {
		for _, at := range grants {
			if at.After(expiresAt) {
				expiresAt = at
			}
		}
	}
	return loginRole(o.params.Role), expiresAt
}

// command connects to the instance through the IAP tunnel.
func (o *operation) command() string {
	return fmt.Sprintf("gcloud compute ssh %s --zone=%s --project=%s --tunnel-through-iap", o.params.Instance, o.params.Zone, o.project)
}

func summary(instance *compute.Instance) *Summary {
	s := &Summary{Name: instance.GetName(), State: instance.GetStatus()}
	if accounts := instance.GetServiceAccounts(); len(accounts) > 0 {
		s.ServiceAccount = accounts[0].GetEmail()
	}
	return s
}
