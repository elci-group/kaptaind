function checkout(c::Cart)
    validate(c)
    audit(c)
    return c.total
end
