-module(change_arity).
-export([connect/1]).

connect(Host) ->
    {Host, 8080}.
