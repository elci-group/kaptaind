-- Top-level (non-local) functions are treated as public API.
function greet(name)
    return "hello " .. name
end

function sum(a, b)
    return a + b
end
