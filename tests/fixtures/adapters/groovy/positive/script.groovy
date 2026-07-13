#!/usr/bin/env groovy

def greet(String name) {
    return "Hello, ${name}"
}

def double(int n) {
    return n * 2
}

println greet("world")
println double(21)
