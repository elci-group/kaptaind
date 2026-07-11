// Scoped/private access modifiers must NOT be public API.
package example.negative

private class Service {
  private[this] def localOnly(): Unit = ()
  private[example] def pkgOnly(): Unit = ()
  protected[this] def protLocal(): Unit = ()
}

private[example] class PackagePrivate

protected[example] trait Internal
