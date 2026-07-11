fn private_fn() {}

struct PrivateStruct;

enum PrivateEnum {
    A,
    B,
}

const PRIVATE_CONST: u32 = 1;
type PrivateAlias = u32;

struct Holder;
impl Holder {
    fn private_method(&self) {}
}
