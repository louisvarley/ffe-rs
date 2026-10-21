//! Abstract original depth groups and sort order, not raster commands or a byte allocator.

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry<T> {
    key: u32,
    values: Vec<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order<T> {
    entries: Vec<Entry<T>>,
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
            entries: Vec::new(),
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

    /// F1780: close the preceding group and optionally open another, even if empty.
    pub fn group(&mut self, key: Option<u32>) -> Result<(), String> {
        self.advance()?;
        self.active = key.map(|key| {
            self.entries.push(Entry {
                key,
                values: Vec::new(),
            });
            self.entries.len() - 1
        });
        Ok(())
    }

    /// F1778: an active group's primitives stay contiguous and ignore their own keys.
    pub fn push(&mut self, key: u32, value: T) -> Result<(), String> {
        self.advance()?;
        if let Some(index) = self.active {
            self.entries[index].values.push(value);
        } else {
            self.entries.push(Entry {
                key,
                values: vec![value],
            });
        }
        Ok(())
    }

    /// F1788 uses descending unsigned keys; ties prefer the earlier allocation.
    /// An open group is an error, not an implicit terminator absent from the stream.
    pub fn finish(self) -> Result<Vec<T>, String> {
        Ok(self
            .finish_with_keys()?
            .into_iter()
            .map(|(_, value)| value)
            .collect())
    }

    /// Same ordering as [`Self::finish`], retaining each value's effective key.
    /// Group members inherit the group key; standalone values keep their own key.
    pub fn finish_with_keys(mut self) -> Result<Vec<(u32, T)>, String> {
        if self.active.is_some() {
            return Err("Depth-order group is still open.".to_owned());
        }
        self.entries
            .sort_by_key(|entry| std::cmp::Reverse(entry.key));
        Ok(self
            .entries
            .into_iter()
            .flat_map(|entry| {
                let key = entry.key;
                entry.values.into_iter().map(move |value| (key, value))
            })
            .collect())
    }
}
