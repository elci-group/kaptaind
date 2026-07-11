package com.example;

public class Box<T> {
    public <U> U echo(U value) {
        return value;
    }

    public T get() {
        return null;
    }
}
