package Helpers;

sub public_api {
    return 1;
}

# Perl convention: a leading underscore marks a sub as internal/private.
# The adapter has NO underscore filter, so both of these are emitted.
sub _private_helper {
    return 2;
}

sub _another_internal {
    return 3;
}

1;
