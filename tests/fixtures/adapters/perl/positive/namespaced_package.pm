package Acme::Widget::Factory::Builder;

use constant VERSION_TAG => 'v1';

sub build {
    return bless {}, __PACKAGE__;
}

1;
