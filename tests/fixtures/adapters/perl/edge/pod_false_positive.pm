package Documented;

sub real_api {
    return 1;
}

=pod

=head1 Examples

    sub frobnicate { ... }   # verbatim POD example line

The adapter does not skip POD, so the indented line above (which trims
to "sub frobnicate") is emitted as a public sub — a false positive.

=cut

1;
