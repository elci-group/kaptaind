function register(
    name::String,
    weights::Dict{String, Int},
)
    return name
end

lookup(
    d::Dict{String, Int},
    key::String,
) = d[key]
