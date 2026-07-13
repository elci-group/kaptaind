submit <- function(order) {
  validate(order)
  audit(order)
  order$id
}
