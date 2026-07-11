// Public defs (kind 'def'), Scala 3 top-level + generic + override.
package example.positive

def greet(name: String): String = s"hello $name"

def identity[A](a: A): A = a

class Impl extends Logger {
  override def log(msg: String): Unit = println(msg)
}
