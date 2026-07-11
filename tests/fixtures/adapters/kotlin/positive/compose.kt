package com.example.ui

import androidx.compose.runtime.Composable

@Composable
fun Greeting(name: String) {
    Text(text = "Hello $name")
}

object Bridge {
    @JvmStatic
    fun fromJava(): String = "hi"

    @JvmField
    val DEFAULT = 42
}
