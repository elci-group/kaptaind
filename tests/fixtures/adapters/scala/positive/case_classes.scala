// Public case classes (kind 'case_class', checked before plain 'class').
package example.positive

case class User(name: String, age: Int)

case class Point[A](x: A, y: A)

final case class Id(value: Long)
