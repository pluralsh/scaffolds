package cloudsql

// maxNameLength is the longest instance name Cloud SQL accepts. Together with the project ID
// and a colon it must also fit in 98 characters, which Cloud SQL checks itself.
const maxNameLength = 98

// InstanceName is the name of a Cloud SQL instance, without the project.
type InstanceName string

// Valid reports whether the name is a Cloud SQL instance name: lowercase letters, digits and
// hyphens, starting with a letter and not ending with a hyphen.
func (n InstanceName) Valid() bool {
	if n == "" || len(n) > maxNameLength || n[0] < 'a' || n[0] > 'z' || n[len(n)-1] == '-' {
		return false
	}
	for i := range len(n) {
		if b := n[i]; (b < 'a' || b > 'z') && (b < '0' || b > '9') && b != '-' {
			return false
		}
	}
	return true
}
