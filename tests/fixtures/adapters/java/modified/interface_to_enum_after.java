package com.example;

public enum Status {
    OK,
    ERROR,
    PENDING;

    public String label() {
        return name();
    }
}
