// AFTER: public def 'sub' was removed -> removal non-empty -> breaking.
package example.breaking

object Math {
  def add(x: Int, y: Int): Int = x + y
}
