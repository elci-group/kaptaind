class OrderService {
    def submit(Order order) {
        return order.id
    }

    private def rollback() {
        return false
    }
}
