-module(no_export_attribute).

%% No -export attribute at all: nothing is public API.
start() -> ok.

stop(_Reason) -> ok.
