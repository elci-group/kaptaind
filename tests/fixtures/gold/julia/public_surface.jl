module Geometry

const VERSION = "1.0"

abstract type Shape end

struct Point
    x::Float64
    y::Float64
end

function distance(a::Point, b::Point)
    return sqrt((a.x - b.x)^2 + (a.y - b.y)^2)
end

area(p::Point) = 0.0

macro logged(expr)
    return expr
end

end
