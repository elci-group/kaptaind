class OrderService {
    def submit(Order order) {
        return order.id
    }

    def cancel(String id) {
        return true
    }
}
