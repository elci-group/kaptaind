#!/usr/bin/env perl
use strict;
use warnings;

sub greet {
    my ($name) = @_;
    return "hello $name";
}

sub main {
    print greet("world"), "\n";
}

main();
