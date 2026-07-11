(ns sample.proto)

(defprotocol Greeter
  (greet [this] "Say hello")
  (farewell [this]))
