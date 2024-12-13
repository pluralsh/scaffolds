# Gatekeeper

This is a baseline, prod-ready OPA Gatekeeper installation using Plural. Besides Gatekeeper installation, it includes a policy bundle and set of constraints.

You might want to slightly tweak the default setup for a few reasons:

- only want to set up policy enforcement on a subset of your fleet (it's fleet-wide by default)
- prefer to choose a different policy bundle
- tweaking namespace names, crd names, etc. for your organization's preferences

## Contributing

If there are any features or documentation you'd like to add to this setup, please feel free to contribute back at https://github.com/pluralsh/scaffolds.