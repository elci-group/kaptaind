function checkout(c::Cart)
    return c.total
end

function refund(c::Cart)
    return -c.total
end
