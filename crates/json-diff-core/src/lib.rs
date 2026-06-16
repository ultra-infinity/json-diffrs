use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

pub enum WrappedValue<'a> {
    Object(BTreeMap<String, WrappedValue<'a>>),
    Array(Vec<WrappedValue<'a>>),
    Value(DiffNode<'a>),
}

pub enum NodeStatus {
    Unchanged,
    Modified,
    Added,
    Deleted,
}

pub struct DiffNode<'a> {
    pub value: &'a Value,
    pub status: NodeStatus,
}

impl<'a> From<&'a Value> for DiffNode<'a> {
    fn from(value: &'a Value) -> Self {
        Self {
            value,
            status: NodeStatus::Unchanged,
        }
    }
}

pub fn gen_diff_json<'a>(
    old_json: &'a BTreeMap<String, Value>,
    new_json: &'a BTreeMap<String, Value>,
) -> BTreeMap<String, WrappedValue<'a>> {
    let old_keys = old_json.keys().into_iter();
    let new_keys = new_json.keys().into_iter();
    let mut map = BTreeMap::new();
    let all_keys: BTreeSet<_> = old_keys.chain(new_keys).collect();

    for key in all_keys {
        let old_val = old_json.get(key);
        let new_val = new_json.get(key);
        unimplemented!()
    }

    map
}
