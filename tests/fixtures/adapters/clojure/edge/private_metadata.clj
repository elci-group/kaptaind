(ns sample.priv)

(defn ^:private hidden [x]
  (* x x))

(def ^:private secret 42)
