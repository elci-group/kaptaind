module Orders

const MAX_RETRIES = 3

abstract type Order end

struct Cart
    id::Int
    total::Float64
end

function checkout(c::Cart)
    return c.total
end

function _validate(c::Cart)
    return true
end

end
