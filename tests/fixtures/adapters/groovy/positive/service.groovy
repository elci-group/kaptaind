class OrderService {
    String region
    int retries = 3
    private Connection conn

    OrderService(String region) {
        this.region = region
    }

    def submit(Order order) {
        return order.id
    }

    Order find(String id) {
        return null
    }

    static void audit(String message) {
        println message
    }

    private def rollback() {
        return false
    }
}
