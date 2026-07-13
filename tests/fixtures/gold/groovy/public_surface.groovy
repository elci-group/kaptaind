class Greeter {
    String name
    private int count

    Greeter(String name) {
        this.name = name
    }

    def greet(String target) {
        return "Hello, ${target}"
    }

    private def reset() {
        count = 0
    }
}

trait Loggable {
    def log(String message) {
        println message
    }
}
