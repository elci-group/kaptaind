class OrderService {
    def submit(Order order) {
        validate(order)
        return order.id
    }
}
