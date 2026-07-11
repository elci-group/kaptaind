// The adapter scans line-by-line with no brace/string scope:
// a local def inside a method and a decl inside a triple-quoted string
// are BOTH flagged as public (over-detection / false positives).
package example.edge

object Demo {
  def outer(): Unit = {
    def inner(): Int = 42
    println(inner())
  }
}

val doc = """
  class Phantom
"""
