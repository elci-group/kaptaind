class User {
  String name;
  User(this.name);
}

abstract class Repository {
  Future<void> load();
}

sealed class Shape {}

final class ImmutablePoint {
  final int x;
}

base class Engine {}

interface class Drawable {}

mixin class MixinClass {}
