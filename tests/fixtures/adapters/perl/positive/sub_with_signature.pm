package Demo::Signed;

sub with_signature($self, $value) {
    return $value;
}

sub as_method($self) :method {
    return $self;
}

sub plain {
    return 1;
}

1;
