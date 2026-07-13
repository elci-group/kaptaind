-- function old_add(a, b)
--   return 0
-- end

--[[
function legacy_mul(a, b)
  return a * b
end
]]

local M = {}

function add(a, b)
  return a + b
end

M.run = function()
  return true
end

return M
