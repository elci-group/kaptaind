# function fake_hash(x) = x

#=
struct FakeBlock
    a::Int
end
=#

"""
function doc_fake(x)
    return x
end
"""

struct Real
    v::Int
end

genuine(p::Real) = p.v
