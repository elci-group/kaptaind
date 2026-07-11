(ns sample.macros)

(defmacro when-let*
  [bindings & body]
  `(let ~bindings ~@body))
