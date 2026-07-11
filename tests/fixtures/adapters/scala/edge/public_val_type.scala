// Public vals/vars/type aliases are part of the API but the adapter only
// recognizes class/case class/object/trait/def -> these are MISSED.
package example.edge

object Api {
  val version: String = "1.0"
  var counter: Int = 0
  type UserId = Long
}
