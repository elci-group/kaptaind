-module(zero_arity).
-export([start/0, version/0]).

%% Zero-arity exports must be emitted as name/0.
start() -> ok.

version() -> <<"1.0.0">>.
