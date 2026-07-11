package com.example;

public enum Level {
    LOW,
    MEDIUM,
    HIGH;

    public int rank() {
        return ordinal();
    }
}
