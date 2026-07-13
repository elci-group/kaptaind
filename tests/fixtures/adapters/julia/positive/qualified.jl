struct Token
    value::String
end

function Base.show(io::IO, t::Token)
    print(io, t.value)
end

Base.length(t::Token) = length(t.value)
