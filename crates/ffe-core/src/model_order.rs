//! Abstract original depth groups and sort order, not raster commands or a byte allocator.

#[derive(Debug, Clone, PartialEq, Eq)]
enum Content<T> {
    Values(Vec<T>),
    /// F1781/F1782 nested block: its own list, sorted when the block is drawn.
    Nested(Vec<Entry<T>>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry<T> {
    key: u32,
    content: Content<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order<T> {
    /// The root list, then one list per open nested block (DATA_009254 stack).
    lists: Vec<Vec<Entry<T>>>,
    /// Keys of the open nested blocks, aligned with `lists[1..]`.
    nested: Vec<u32>,
    active: Option<usize>,
    operations: usize,
    budget: usize,
}

impl<T> Order<T> {
    /// This development budget counts group changes and primitive insertions,
    /// not the original allocator's 50,000-word limit.
    pub fn new(budget: usize) -> Result<Self, String> {
        if !(1..=4096).contains(&budget) {
            return Err("Depth-order budget must be 1..4096 operations.".to_owned());
        }
        Ok(Self {
            lists: vec![Vec::new()],
            nested: Vec::new(),
            active: None,
            operations: 0,
            budget,
        })
    }

    fn advance(&mut self) -> Result<(), String> {
        if self.operations == self.budget {
            return Err("Depth-order operation budget exhausted.".to_owned());
        }
        self.operations += 1;
        Ok(())
    }

    fn current(&mut self) -> &mut Vec<Entry<T>> {
        self.lists.last_mut().expect("the root list always exists")
    }

    /// F1780: close the preceding group and optionally open another, even if empty.
    pub fn group(&mut self, key: Option<u32>) -> Result<(), String> {
        self.advance()?;
        self.active = key.map(|key| {
            let list = self.current();
            list.push(Entry {
                key,
                content: Content::Values(Vec::new()),
            });
            list.len() - 1
        });
        Ok(())
    }

    /// F1781 (`Some(key)`): close the group, queue a block keyed `key` in the current
    /// list and make its own list current. F1782 (`None`): close the group and return
    /// to the enclosing list.
    pub fn nested(&mut self, key: Option<u32>) -> Result<(), String> {
        self.advance()?;
        self.active = None;
        match key {
            Some(key) => {
                self.lists.push(Vec::new());
                self.nested.push(key);
            }
            None => {
                let (Some(key), true) = (self.nested.pop(), self.lists.len() > 1) else {
                    return Err("Nested depth block closed without one open.".to_owned());
                };
                let list = self.lists.pop().expect("checked above");
                self.current().push(Entry {
                    key,
                    content: Content::Nested(list),
                });
            }
        }
        Ok(())
    }

    /// F1778: an active group's primitives stay contiguous and ignore their own keys.
    pub fn push(&mut self, key: u32, value: T) -> Result<(), String> {
        self.advance()?;
        let active = self.active;
        let list = self.current();
        match active.map(|index| &mut list[index].content) {
            Some(Content::Values(values)) => values.push(value),
            _ => list.push(Entry {
                key,
                content: Content::Values(vec![value]),
            }),
        }
        Ok(())
    }

    /// F1788 uses descending unsigned keys; ties prefer the earlier allocation.
    /// An open group or nested block is an error, not an implicit terminator.
    pub fn finish(self) -> Result<Vec<T>, String> {
        Ok(self
            .finish_with_keys()?
            .into_iter()
            .map(|(_, value)| value)
            .collect())
    }

    /// Same ordering as [`Self::finish`], retaining each value's effective key.
    /// Group members inherit the group key; standalone values keep their own key; a
    /// nested block's members are sorted within the block (F1790) and keep theirs.
    pub fn finish_with_keys(mut self) -> Result<Vec<(u32, T)>, String> {
        if self.active.is_some() {
            return Err("Depth-order group is still open.".to_owned());
        }
        if self.lists.len() > 1 {
            return Err("Nested depth block is still open.".to_owned());
        }
        let mut output = Vec::new();
        flatten(self.lists.pop().expect("root list"), &mut output);
        Ok(output)
    }
}

fn flatten<T>(mut entries: Vec<Entry<T>>, output: &mut Vec<(u32, T)>) {
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.key));
    for entry in entries {
        match entry.content {
            Content::Values(values) => output.extend(values.into_iter().map(|v| (entry.key, v))),
            Content::Nested(list) => flatten(list, output),
        }
    }
}
