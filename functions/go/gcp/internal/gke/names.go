package gke

import "github.com/pluralsh/scaffolds/functions/go/gcp/internal/compute"

// maxNameLength is the longest cluster or node pool name GKE accepts.
const maxNameLength = 40

// Location is where a cluster runs: a region for a regional cluster, a zone for a zonal one.
type Location string

// Valid reports whether the location is a region, e.g. `us-central1`, or a zone, e.g.
// `us-central1-a`.
func (l Location) Valid() bool {
	return compute.Zone(l).Valid() || compute.Zone(l+"-a").Valid()
}

// Name is the name of a cluster or node pool.
type Name string

// Valid reports whether the name is a GKE cluster or node pool name: lowercase letters,
// digits and hyphens, starting with a letter, not ending with a hyphen, at most 40 characters.
func (n Name) Valid() bool {
	return len(n) <= maxNameLength && compute.ResourceName(n).Valid()
}
