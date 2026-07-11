package com.example.app

val template = """
    fun ghost() {}
    class Phantom {}
""".trimIndent()

fun real(): String = template
