-module(multiline_export).
-export([
    start/0,
    stop/0
]).

%% The adapter only reads the -export list up to the first ']' on the SAME
%% line, so a multi-line export list leaves the export set empty and these
%% exported functions are NOT detected (documented miss).
start() -> ok.

stop() -> ok.
