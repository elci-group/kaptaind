// Private / protected members must NOT be public API.
package example.negative

private class Service {
  private def internal(): Unit = ()
  protected def hook(): Unit = ()
  private val secret = 42
}

private object Helper

protected class Shielded
