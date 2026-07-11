package edge

// Map returns the keys of m.
func Map[K comparable, V any](m map[K]V) []K {
	keys := make([]K, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	return keys
}

// Stack is a generic LIFO container.
type Stack[T any] struct {
	items []T
}
