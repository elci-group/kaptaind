-module(my_mod).
-export([start/0]).
-record(state, {count :: integer()}).
-define(MAX, 100).

start() -> ok.

private_helper() -> ok.
