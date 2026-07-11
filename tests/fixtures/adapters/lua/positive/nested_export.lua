local M = {}
M.deep = {}

M.deep.run = function()
    return true
end

function M.deep.walk()
    return false
end

return M
