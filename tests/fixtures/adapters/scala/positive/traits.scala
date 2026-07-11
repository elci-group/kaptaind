// Public traits (kind 'trait'), including modifier-prefixed forms.
package example.positive

trait Logger {
  def log(msg: String): Unit
}

sealed trait Shape

trait Functor[F[_]] {
  def map[A, B](fa: F[A])(f: A => B): F[B]
}
