(ns sample.proto)

(defprotocol Storage
  (put! [this k v]))

(defn helper [] :ok)
