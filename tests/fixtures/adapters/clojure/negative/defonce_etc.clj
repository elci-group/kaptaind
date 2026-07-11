(ns sample.other)

(defonce cache (atom {}))

(defmulti area :shape)
(defmethod area :circle [c] (* Math/PI (:r c) (:r c)))

(deftype Point [x y])
(defrecord Person [name age])
