-- `M` is local and never returned; the adapter does not check scope/return.
local M = {}

M.hidden = 1

function M.secret()
    return "leaked"
end
