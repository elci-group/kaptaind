package com.example.app

fun greet(name: String): String {
    return "Hello, $name"
}

fun add(a: Int, b: Int): Int = a + b

suspend fun loadUser(id: Long): User {
    return repo.fetch(id)
}
