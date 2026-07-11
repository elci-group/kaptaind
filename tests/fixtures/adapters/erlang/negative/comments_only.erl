-module(comments_only).
-export([]).

% -export([ghost/1]).
% -record(ghost, {x}).
% -define(GHOST, 1).

%% Not exported, so not public even though a comment mentions it.
ghost(X) -> X.
