package positive

// Config holds server settings.
type Config struct {
	Host string
	Port int
}

// NewConfig builds a default Config.
func NewConfig() Config {
	return Config{Host: "localhost", Port: 8080}
}
