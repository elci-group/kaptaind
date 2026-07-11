// pub fn commented_out() {}

/*
pub fn block_commented() {}
pub struct BlockStruct;
*/

fn real_private() {
    let _s = "pub fn inside_string() {}";
    let _r = r"pub fn raw_string() {}";
}

const _DOC: &str = "pub fn in_const_string() {}";
