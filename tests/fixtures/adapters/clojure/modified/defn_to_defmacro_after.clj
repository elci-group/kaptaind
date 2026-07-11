(ns sample.core)

(defmacro greet [name]
  `(str "Hello, " ~name))
