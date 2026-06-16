use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

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

fn gen_diff_object<'a>(
    old_object: &'a Map<String, Value>,
    new_object: &'a Map<String, Value>,
) -> WrappedValue<'a> {
    unimplemented!()
}

fn gen_diff_array<'a>(old_array: &'a Vec<Value>, new_array: &'a Vec<Value>) -> WrappedValue<'a> {
    unimplemented!()
}

fn gen_diff_value<'a>(old_value: &'a Value, new_value: &'a Value) -> WrappedValue<'a> {
    match (old_value, new_value) {
        (Value::Object(old_object), Value::Object(new_object)) => {
            gen_diff_object(old_object, new_object)
        }
        (Value::Array(old_array), Value::Array(new_array)) => gen_diff_array(old_array, new_array),
        (Value::Bool(old_bool), Value::Bool(new_bool)) => {
            let mut diff_node = DiffNode::from(new_value);
            if old_bool != new_bool {
                diff_node.status = NodeStatus::Modified;
            }

            WrappedValue::Value(diff_node)
        }
        (Value::Number(old_number), Value::Number(new_number)) => {
            let mut diff_node = DiffNode::from(new_value);
            if old_number != new_number {
                diff_node.status = NodeStatus::Modified;
            }

            WrappedValue::Value(diff_node)
        }
        (Value::String(old_string), Value::String(new_string)) => {
            let mut diff_node = DiffNode::from(new_value);
            if old_string != new_string {
                diff_node.status = NodeStatus::Modified;
            }

            WrappedValue::Value(diff_node)
        }
        (Value::Null, Value::Null) => WrappedValue::Value(DiffNode {
            value: old_value,
            status: NodeStatus::Unchanged,
        }),
        (value, _) => WrappedValue::Value(DiffNode {
            value,
            status: NodeStatus::Modified,
        }),
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

        let value = match (old_val, new_val) {
            (Some(old_val), Some(new_val)) => gen_diff_value(old_val, new_val),
            (Some(value), None) => WrappedValue::Value(DiffNode {
                value,
                status: NodeStatus::Deleted,
            }),
            (None, Some(value)) => WrappedValue::Value(DiffNode {
                value,
                status: NodeStatus::Added,
            }),
            (None, None) => unreachable!(),
        };

        map.insert(key.clone(), value);
    }

    map
}
