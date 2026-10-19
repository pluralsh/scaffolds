package volume

import "time"

// ParseTimestamp parses an RFC 3339 timestamp, as used by the GCP and Azure APIs, into Unix
// seconds.
func ParseTimestamp(value string) (int64, bool) {
	t, err := time.Parse(time.RFC3339, value)
	if err != nil {
		return 0, false
	}
	return t.Unix(), true
}
