package com.example.app

interface Repository {
    fun fetch(id: Long): User
}

object Config {
    const val NAME = "app"
}

annotation class Serializable(val kind: String)
