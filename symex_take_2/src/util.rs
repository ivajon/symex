use core::range::Range;
use std::collections::BTreeMap;

use tracing::Value;

enum Comparison<T: Sized> {
    G(T),
    L(T),
}

struct RangeTreeNode<Key: Sized + PartialOrd + Ord, Value: Sized> {
    // NOTE: Stupid simple solution as to not have to deal with leaky lifetimes.
    key: (Key, Key),
    values: Vec<Value>,
    parent: Option<core::ptr::NonNull<Box<Self>>>,
    left: Option<core::ptr::NonNull<Box<Self>>>,
    right: Option<core::ptr::NonNull<Box<Self>>>,
}
pub struct RangeTree<Key: Sized + PartialOrd + Ord, Value: Sized> {
    nodes: Vec<Box<RangeTreeNode<Key, Value>>>,
    middle: usize,
}

impl<Key: Sized + PartialOrd + Ord, Value: Sized> RangeTree<Key, Value> {
    fn insert(&mut self, node: RangeTree<Key, Value>) {}

    fn insert_internal(
        &mut self,
        current: &mut Box<RangeTreeNode<Key, Value>>,
        node: &mut Box<RangeTreeNode<Key, Value>>,
        level: usize,
    ) {
        let cmp = Self::cmp(current, node, level);
        match cmp {
            std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => {
                // Go right
                if current.right.is_none() {
                    current.right = Some(core::ptr::NonNull::from_ref(node));
                } else {
                    self.insert_internal(
                        unsafe { current.right.unwrap().as_mut() },
                        node,
                        level + 1,
                    );
                }
            }
            std::cmp::Ordering::Less => {
                // Go left
                if current.left.is_none() {
                    current.left = Some(core::ptr::NonNull::from_ref(node));
                } else {
                    self.insert_internal(
                        unsafe { current.left.unwrap().as_mut() },
                        node,
                        level + 1,
                    );
                }
            }
        }
    }

    fn cmp(
        current: &mut RangeTreeNode<Key, Value>,
        node: &mut RangeTreeNode<Key, Value>,
        level: usize,
    ) -> std::cmp::Ordering {
        let side = (level % 2) == 0;

        if side {
            current.key.0.cmp(&node.key.0)
        } else {
            current.key.1.cmp(&node.key.1)
        }
    }

    fn avl_rotation(node: &mut RangeTreeNode<Key, Value>, level: usize) {}
}

impl<Key: Sized + PartialOrd + Ord, Value: Sized, Source: Iterator<Item = (Key, Key, Value)>>
    From<Source> for RangeTree<Key, Value>
{
    fn from(value: Source) -> Self {
        let data = value.collect::<Vec<(Key, Key, Value)>>();
    }
}
