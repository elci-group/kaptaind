package com.example.app

fun <T> identity(value: T): T = value

fun <T : Comparable<T>> maxOf(a: T, b: T): T = if (a >= b) a else b

class Box<T>(val value: T)

interface Mapper<In, Out> {
    fun map(input: In): Out
}
