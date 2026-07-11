package breaking

// Greet formats a greeting; loud uppercases it.
func Greet(name string, loud bool) string {
	if loud {
		return "HI " + name
	}
	return "hi " + name
}
