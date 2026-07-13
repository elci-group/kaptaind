#!/usr/bin/env groovy

def x = 5
def name = "world"
println "hello ${name}"
println(x)
assert x > 0
def result = compute(x, name)
items.each { println it }
