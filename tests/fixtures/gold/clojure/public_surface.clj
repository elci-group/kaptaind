(ns myapp.core)

(defn add [a b]
  (+ a b))

(defmacro unless [pred body]
  `(if (not ~pred) ~body))

(defprotocol Store
  (save [this v]))

(def VERSION "1.0")
