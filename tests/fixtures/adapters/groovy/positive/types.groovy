interface Repository {
    def find(String id)

    def save(Entity entity)
}

trait Timestamped {
    long createdAt

    def touch() {
        createdAt = System.currentTimeMillis()
    }
}

enum Status {
    OPEN,
    CLOSED
}

@interface Audited {
}
