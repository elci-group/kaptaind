area(p) = 0.0

double(x::Int) = 2x

mutable struct Vec{T}
    x::T
    y::T
end

macro timed(expr)
    return :(time($expr))
end

greet!(name) = println("hi ", name)
