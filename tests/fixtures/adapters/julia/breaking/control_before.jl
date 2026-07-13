function checkout(c::Cart)
    validate(c)
    return c.total
end
