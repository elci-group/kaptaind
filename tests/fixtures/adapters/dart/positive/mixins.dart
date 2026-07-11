mixin Logging {
  void log(String msg) {}
}

mixin Jsonable on Object {
  Map<String, dynamic> toJson() => {};
}
