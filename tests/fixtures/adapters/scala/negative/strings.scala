// Keywords inside single-line string literals must NOT be flagged.
// (Scala 3 top-level vals; lines start with 'val', never a decl keyword.)
package example.negative

val a = "class NotAClass"
val b = "def notADef(): Unit = ()"
val c = "trait NotATrait"
val d = "case class NotACase(x: Int)"
val e = "object NotAnObject"
