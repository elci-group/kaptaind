local M = {}

function M.add(a, b)
    return a + b
end

function M.sub(a, b)
    return a - b
end

function M:reset()
    self.value = 0
end

return M
