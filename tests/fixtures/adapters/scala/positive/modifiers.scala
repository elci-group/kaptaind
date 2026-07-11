// Leading modifiers are stripped; the underlying public decl still flags.
package example.positive

sealed abstract class Animal

abstract class Living

final object Util

open class OpenBase

inline def twice(x: Int): Int = x * 2
