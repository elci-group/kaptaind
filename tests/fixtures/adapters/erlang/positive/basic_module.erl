-module(basic_module).
-export([start/0, stop/1]).

-record(state, {count :: integer()}).
-define(MAX_LIMIT, 100).

%% Public: starts the server.
start() -> ok.

%% Public: stops the server with a reason.
stop(_Reason) -> ok.

%% Private: not in the -export list.
private_helper() -> ok.
