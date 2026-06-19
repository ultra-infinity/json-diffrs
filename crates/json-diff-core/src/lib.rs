use egui_json_tree::pointer::JsonPointerSegment;
use egui_json_tree::value::BaseValueType;
use egui_json_tree::value::ExpandableType;
use egui_json_tree::value::JsonTreeValue;
use egui_json_tree::value::ToJsonTreeValue;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::fmt::Display;

#[derive(Debug)]
pub enum WrappedValue<'a> {
    Object(BTreeMap<String, WrappedValue<'a>>),
    Array(Vec<WrappedValue<'a>>),
    Base(DiffNode<'a>),
}

impl<'a> WrappedValue<'a> {
    pub fn status(&self) -> NodeStatus {
        match self {
            WrappedValue::Base(node) => node.status,
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

impl Display for NodeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use NodeStatus::*;
        let text = match self {
            Unchanged => "unchanged",
            Modified => "modified",
            Added => "added",
            Deleted => "deleted",
        };

        f.write_str(text)
    }
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

#[derive(Debug)]
pub struct DiffNode<'a> {
    pub value: &'a Value,
    pub status: NodeStatus,
}

impl<'a> Display for DiffNode<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self.value, f)?;
        Display::fmt(&self.status, f)
    }
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
            (Some(old_value), None) => WrappedValue::Base(DiffNode {
                value: old_value,
                status: NodeStatus::Deleted,
            }),
            (None, Some(new_value)) => WrappedValue::Base(DiffNode {
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
            (Some(old_value), None) => WrappedValue::Base(DiffNode {
                value: old_value,
                status: NodeStatus::Deleted,
            }),
            (None, Some(new_value)) => WrappedValue::Base(DiffNode {
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

            WrappedValue::Base(diff_node)
        }
        (Value::Number(old_number), Value::Number(new_number)) => {
            let mut diff_node = DiffNode::from(new_value);
            if old_number != new_number {
                diff_node.status = NodeStatus::Modified;
            }

            WrappedValue::Base(diff_node)
        }
        (Value::String(old_string), Value::String(new_string)) => {
            let mut diff_node = DiffNode::from(new_value);
            if old_string != new_string {
                diff_node.status = NodeStatus::Modified;
            }

            WrappedValue::Base(diff_node)
        }
        (Value::Null, Value::Null) => WrappedValue::Base(DiffNode {
            value: old_value,
            status: NodeStatus::Unchanged,
        }),
        (_, value) => WrappedValue::Base(DiffNode {
            value,
            status: NodeStatus::Modified,
        }),
    }
}

pub fn gen_diff_json<'a>(
    old_json: &'a BTreeMap<String, Value>,
    new_json: &'a BTreeMap<String, Value>,
) -> WrappedValue<'a> {
    let old_keys = old_json.keys().into_iter();
    let new_keys = new_json.keys().into_iter();
    let mut map = BTreeMap::new();
    let all_keys: BTreeSet<_> = old_keys.chain(new_keys).collect();

    for key in all_keys {
        let old_val = old_json.get(key);
        let new_val = new_json.get(key);

        let value = match (old_val, new_val) {
            (Some(old_val), Some(new_val)) => gen_diff_value(old_val, new_val),
            (Some(value), None) => WrappedValue::Base(DiffNode {
                value,
                status: NodeStatus::Deleted,
            }),
            (None, Some(value)) => WrappedValue::Base(DiffNode {
                value,
                status: NodeStatus::Added,
            }),
            (None, None) => unreachable!(),
        };

        map.insert(key.clone(), value);
    }

    WrappedValue::Object(map)
}

impl<'a> ToJsonTreeValue for WrappedValue<'a> {
    fn to_json_tree_value(&self) -> JsonTreeValue<'_, Self> {
        use WrappedValue::*;
        match self {
            Object(obj) => {
                let mut res = Vec::new();
                let expandable_type = ExpandableType::Object;

                for (key, value) in obj {
                    let pointer_segment = JsonPointerSegment::Key(key);
                    res.push((pointer_segment, value));
                }

                JsonTreeValue::Expandable(res, expandable_type)
            }
            Array(array) => {
                let mut res = Vec::new();
                let expandable_type = ExpandableType::Object;

                for (index, value) in array.iter().enumerate() {
                    let pointer_segment = JsonPointerSegment::Index(index);
                    res.push((pointer_segment, value));
                }

                JsonTreeValue::Expandable(res, expandable_type)
            }
            Base(node) => {
                let DiffNode { value, .. } = node;
                match value {
                    Value::Null => JsonTreeValue::Base(self, value, BaseValueType::Null),
                    Value::Bool(value) => JsonTreeValue::Base(self, value, BaseValueType::Bool),
                    Value::Number(number) => {
                        JsonTreeValue::Base(self, number, BaseValueType::Number)
                    }
                    Value::String(string) => {
                        JsonTreeValue::Base(self, string, BaseValueType::String)
                    }
                    Value::Array(_) | Value::Object(_) => unreachable!(),
                }
            }
        }
    }

    fn is_expandable(&self) -> bool {
        matches!(self, WrappedValue::Object(_) | WrappedValue::Array(_))
    }
}
