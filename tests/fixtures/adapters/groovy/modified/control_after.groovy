class OrderService {
    def submit(Order order) {
        validate(order)
        audit(order)
        return order.id
    }
}
