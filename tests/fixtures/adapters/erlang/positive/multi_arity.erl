-module(multi_arity).
-export([connect/1, connect/2]).

%% Same name, two arities: both are distinct public symbols.
connect(Host) ->
    connect(Host, 8080).

connect(Host, Port) ->
    {Host, Port}.
