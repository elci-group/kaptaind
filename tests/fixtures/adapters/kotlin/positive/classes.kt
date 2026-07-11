package com.example.app

class ApiClient(private val http: Http) {
    fun call() {}
}

data class User(val id: Long, val name: String)

sealed class Result {
    data class Ok(val value: String) : Result()
    data class Err(val message: String) : Result()
}

enum class Color { RED, GREEN, BLUE }
