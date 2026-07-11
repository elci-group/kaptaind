// BEFORE: object Math exposes two public defs.
package example.breaking

object Math {
  def add(x: Int, y: Int): Int = x + y
  def sub(x: Int, y: Int): Int = x - y
}
