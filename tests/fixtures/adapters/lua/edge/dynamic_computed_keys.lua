local M = {}

-- Computed / string-key exports: invisible to the adapter (only `M.dot` matched).
M["dynamic"] = function()
    return true
end

local key = "computed"
M[key] = 42

return M
