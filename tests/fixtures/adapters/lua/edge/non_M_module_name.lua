-- A module table named anything other than literally `M`.
local MyModule = {}

MyModule.foo = function()
    return 1
end

function MyModule.bar()
    return 2
end

return MyModule
