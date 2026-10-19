package compute

import "strings"

// Zone is the name of a Compute Engine zone.
type Zone string

// Valid reports whether the zone is `<region>-<zone letter>`, e.g. `us-central1-a` or
// `northamerica-northeast1-b`.
func (z Zone) Valid() bool {
	region, letter, ok := strings.CutLast(string(z), "-")
	if !ok {
		return false
	}
	area, location, ok := strings.Cut(region, "-")
	if !ok {
		return false
	}
	lower := func(s string) bool {
		return s != "" && strings.IndexFunc(s, func(r rune) bool { return r < 'a' || r > 'z' }) < 0
	}
	digits := strings.IndexAny(location, "0123456789")
	return lower(area) && len(letter) == 1 && lower(letter) && digits > 0 &&
		lower(location[:digits]) && strings.Trim(location[digits:], "0123456789") == ""
}

// ResourceName is the name of a Compute Engine resource, such as a disk or a snapshot.
type ResourceName string

// Valid reports whether the name is an RFC 1035 name as Compute Engine requires:
// `[a-z]([-a-z0-9]{0,61}[a-z0-9])?`.
func (n ResourceName) Valid() bool {
	if n == "" || len(n) > 63 || n[0] < 'a' || n[0] > 'z' || n[len(n)-1] == '-' {
		return false
	}
	for i := range len(n) {
		if b := n[i]; (b < 'a' || b > 'z') && (b < '0' || b > '9') && b != '-' {
			return false
		}
	}
	return true
}

// NameFromURL is the name at the end of a resource URL, e.g. the instance name of
// `https://www.googleapis.com/compute/v1/projects/p/zones/z/instances/node-1`.
func NameFromURL(url string) string {
	return url[strings.LastIndexByte(url, '/')+1:]
}

// ZoneFromURL is the zone of a zonal resource URL, e.g. `us-central1-a` of
// `https://www.googleapis.com/compute/v1/projects/p/zones/us-central1-a/instanceGroupManagers/g`,
// or empty if the URL has none.
func ZoneFromURL(url string) string {
	_, rest, ok := strings.Cut(url, zonesPath)
	if !ok {
		return ""
	}
	zone, _, _ := strings.Cut(rest, "/")
	return zone
}

// zonesPath is the collection of zones in a resource URL.
const zonesPath = "/zones/"

// RegionFromURL is the region of a regional resource URL, e.g. `us-central1` of
// `https://www.googleapis.com/compute/v1/projects/p/regions/us-central1/targetPools/a`, or
// empty if the URL has none.
func RegionFromURL(url string) string {
	_, rest, ok := strings.Cut(url, regionsPath)
	if !ok {
		return ""
	}
	region, _, _ := strings.Cut(rest, "/")
	return region
}

// regionsPath is the collection of regions in a resource URL.
const regionsPath = "/regions/"
