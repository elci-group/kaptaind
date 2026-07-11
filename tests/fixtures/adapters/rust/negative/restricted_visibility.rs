pub(crate) fn crate_only() {}

pub(super) fn parent_only() {}

pub(in crate::foo) fn scoped() {}

pub(crate) struct Internal;

pub(super) enum AlsoInternal {
    A,
}

mod foo {}
