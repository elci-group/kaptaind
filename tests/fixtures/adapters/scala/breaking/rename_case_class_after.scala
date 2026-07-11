// AFTER: User renamed to UserV2 -> old name removed -> breaking (name-based diff).
package example.breaking

case class UserV2(name: String)

case class Order(id: Long)
