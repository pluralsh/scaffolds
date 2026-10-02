package iap

import (
	"context"
	"fmt"

	iapv1 "google.golang.org/api/iap/v1"
	"google.golang.org/api/option"

	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/core"
	"github.com/pluralsh/scaffolds/functions/go/gcp/internal/iam"
)

// apiClient is the [Client] backed by the IAP API client.
type apiClient struct {
	service *iapv1.Service
	project string
}

var _ Client = (*apiClient)(nil)

func newAPIClient(ctx context.Context, project string, opts ...option.ClientOption) (*apiClient, error) {
	service, err := iapv1.NewService(ctx, opts...)
	if err != nil {
		return nil, apiError(err)
	}
	return &apiClient{service: service, project: project}, nil
}

func (c *apiClient) TunnelPolicy(ctx context.Context, zone, instance string) (*iam.Policy, error) {
	policy, err := c.service.V1.GetIamPolicy(c.tunnel(zone, instance), &iapv1.GetIamPolicyRequest{
		Options: &iapv1.GetPolicyOptions{RequestedPolicyVersion: iam.Version},
	}).Context(ctx).Do()
	if err != nil {
		return nil, apiError(err)
	}
	p := &iam.Policy{Etag: policy.Etag}
	for _, b := range policy.Bindings {
		binding := &iam.Binding{Role: b.Role, Members: b.Members}
		if cond := b.Condition; cond != nil {
			binding.Condition = &iam.Condition{Title: cond.Title, Description: cond.Description, Expression: cond.Expression}
		}
		p.Bindings = append(p.Bindings, binding)
	}
	return p, nil
}

func (c *apiClient) SetTunnelPolicy(ctx context.Context, zone, instance string, policy *iam.Policy) error {
	p := &iapv1.Policy{Etag: policy.Etag, Version: iam.Version}
	for _, b := range policy.Bindings {
		binding := &iapv1.Binding{Role: b.Role, Members: b.Members}
		if cond := b.Condition; cond != nil {
			binding.Condition = &iapv1.Expr{Title: cond.Title, Description: cond.Description, Expression: cond.Expression}
		}
		p.Bindings = append(p.Bindings, binding)
	}
	if _, err := c.service.V1.SetIamPolicy(c.tunnel(zone, instance), &iapv1.SetIamPolicyRequest{Policy: p}).Context(ctx).Do(); err != nil {
		return apiError(err)
	}
	return nil
}

// tunnel is the IAP resource name of the TCP tunnel to the instance.
func (c *apiClient) tunnel(zone, instance string) string {
	return fmt.Sprintf("projects/%s/iap_tunnel/zones/%s/instances/%s", c.project, zone, instance)
}

// apiError reports a failed API call as a provider error.
func apiError(err error) error {
	return core.Providerf("IAP API: %v", err)
}
