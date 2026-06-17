use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

pub enum WrappedValue<'a> {
    Object(BTreeMap<String, WrappedValue<'a>>),
    Array(Vec<WrappedValue<'a>>),
    Value(DiffNode<'a>),
}

impl<'a> WrappedValue<'a> {
    pub fn status(&self) -> NodeStatus {
        match self {
            WrappedValue::Value(node) => node.status,
            WrappedValue::Object(map) => {
                if map.is_empty() {
                    NodeStatus::Unchanged
                } else {
                    // Accumulate the status of all children
                    map.values()
                        .map(|v| v.status())
                        .fold(NodeStatus::Unchanged, |acc, s| acc.combine(s))
                }
            }
            WrappedValue::Array(vec) => {
                if vec.is_empty() {
                    NodeStatus::Unchanged
                } else {
                    // Accumulate the status of all elements
                    vec.iter()
                        .map(|v| v.status())
                        .fold(NodeStatus::Unchanged, |acc, s| acc.combine(s))
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    Unchanged,
    Modified,
    Added,
    Deleted,
}

impl NodeStatus {
    // Combines two statuses to determine parent status
    fn combine(self, other: Self) -> Self {
        match (self, other) {
            // If anything is modified, or if we mix additions and deletions, it's Modified
            (NodeStatus::Modified, _) | (_, NodeStatus::Modified) => NodeStatus::Modified,
            (NodeStatus::Added, NodeStatus::Deleted) | (NodeStatus::Deleted, NodeStatus::Added) => {
                NodeStatus::Modified
            }

            // If they match, keep it
            (NodeStatus::Added, NodeStatus::Added) => NodeStatus::Added,
            (NodeStatus::Deleted, NodeStatus::Deleted) => NodeStatus::Deleted,

            // Unchanged yields to structural changes
            (NodeStatus::Unchanged, structural) | (structural, NodeStatus::Unchanged) => structural,
        }
    }
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
    let old_keys = old_object.keys();
    let new_keys = new_object.keys();

    let keys: BTreeSet<_> = old_keys.chain(new_keys).collect();

    let mut map = BTreeMap::new();
    for key in keys {
        let old_value = old_object.get(key);
        let new_value = new_object.get(key);

        let value = match (old_value, new_value) {
            (Some(old_value), Some(new_value)) => gen_diff_value(old_value, new_value),
            (Some(old_value), None) => WrappedValue::Value(DiffNode {
                value: old_value,
                status: NodeStatus::Deleted,
            }),
            (None, Some(new_value)) => WrappedValue::Value(DiffNode {
                value: new_value,
                status: NodeStatus::Added,
            }),
            (None, None) => unreachable!(),
        };

        map.insert(key.clone(), value);
    }

    WrappedValue::Object(map)
}

fn gen_diff_array<'a>(old_array: &'a Vec<Value>, new_array: &'a Vec<Value>) -> WrappedValue<'a> {
    let max_len = std::cmp::max(old_array.len(), new_array.len());
    let mut array = Vec::new();

    for i in 0..max_len {
        let old_value = old_array.get(i);
        let new_value = new_array.get(i);

        let value = match (old_value, new_value) {
            (Some(old_value), Some(new_value)) => gen_diff_value(old_value, new_value),
            (Some(old_value), None) => WrappedValue::Value(DiffNode {
                value: old_value,
                status: NodeStatus::Deleted,
            }),
            (None, Some(new_value)) => WrappedValue::Value(DiffNode {
                value: new_value,
                status: NodeStatus::Added,
            }),
            (None, None) => unreachable!(),
        };

        array.push(value);
    }

    WrappedValue::Array(array)
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
        (_, value) => WrappedValue::Value(DiffNode {
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
