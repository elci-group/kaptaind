package com.example.app

public fun explicitPublic(): String = "visible"

abstract class Base {
    abstract fun step()
}

open class Openable {
    open fun ping() {}
}

inline fun <reified T> cast(value: Any): T = value as T
