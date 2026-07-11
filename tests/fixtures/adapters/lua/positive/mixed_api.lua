local M = {}

-- public export (module_export)
M.ping = function()
    return "pong"
end

-- public method (function)
function M.echo(msg)
    return msg
end

-- private helper (NOT detected: local function)
local function sanitize(s)
    return s
end

return M
