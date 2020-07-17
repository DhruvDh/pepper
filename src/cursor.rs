use std::{
    ops::{Drop, Index, IndexMut},
    slice::SliceIndex,
};

use crate::buffer_position::{BufferPosition, BufferRange};

#[derive(Debug, Default, Clone, Copy)]
pub struct Cursor {
    pub anchor: BufferPosition,
    pub position: BufferPosition,
}

impl Cursor {
    pub fn range(&self) -> BufferRange {
        BufferRange::between(self.anchor, self.position)
    }

    pub fn insert(&mut self, range: BufferRange) {
        self.anchor = self.anchor.insert(range);
        self.position = self.position.insert(range);
    }

    pub fn delete(&mut self, range: BufferRange) {
        self.anchor = self.anchor.delete(range);
        self.position = self.position.delete(range);
    }
}

#[derive(Clone)]
pub struct CursorCollection {
    cursors: Vec<Cursor>,
    main_cursor_index: usize,
}

impl CursorCollection {
    pub fn new() -> Self {
        Self {
            cursors: vec![Cursor::default()],
            main_cursor_index: 0,
        }
    }

    pub fn main_cursor(&self) -> &Cursor {
        &self.cursors[self.main_cursor_index]
    }

    pub fn add_cursor(&mut self, cursor: Cursor) {
        self.main_cursor_index = self.cursors.len();
        self.cursors.push(cursor);
        self.sort_and_merge();
    }

    pub fn collapse_anchors(&mut self) {
        for cursor in &mut self.cursors {
            cursor.anchor = cursor.position;
        }
    }

    pub fn swap_positions_and_anchors(&mut self) {
        for cursor in &mut self.cursors {
            std::mem::swap(&mut cursor.anchor, &mut cursor.position);
        }
    }

    pub fn change_all<F>(&mut self, callback: F)
    where
        F: FnOnce(&mut [Cursor]),
    {
        callback(&mut self.cursors[..]);
        self.sort_and_merge();
    }

    pub fn as_mut(&mut self) -> CursorCollectionMut {
        CursorCollectionMut(self)
    }

    fn sort_and_merge(&mut self) {
        let main_cursor = self.cursors[self.main_cursor_index];
        self.cursors.sort_by_key(|c| c.range().from);
        self.main_cursor_index = self
            .cursors
            .binary_search_by(|c| c.position.cmp(&main_cursor.position))
            .unwrap_or(0);

        let mut i = 0;
        while i < self.cursors.len() {
            let mut range = self.cursors[i].range();
            for j in ((i + 1)..self.cursors.len()).rev() {
                let other_range = self.cursors[j].range();
                if range.contains(other_range.from) {
                    range.to = range.to.max(other_range.to);
                    self.cursors.remove(j);
                    if j <= self.main_cursor_index {
                        self.main_cursor_index -= 1;
                    }
                }
            }

            self.cursors[i] = if self.cursors[i].anchor <= self.cursors[i].position {
                Cursor {
                    anchor: range.from,
                    position: range.to,
                }
            } else {
                Cursor {
                    anchor: range.to,
                    position: range.from,
                }
            };

            i += 1;
        }
    }
}

impl<Idx> Index<Idx> for CursorCollection
where
    Idx: SliceIndex<[Cursor]>,
{
    type Output = Idx::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.cursors[index]
    }
}

pub struct CursorCollectionMut<'a>(&'a mut CursorCollection);

impl<'a, Idx> Index<Idx> for CursorCollectionMut<'a>
where
    Idx: SliceIndex<[Cursor]>,
{
    type Output = Idx::Output;

    fn index(&self, index: Idx) -> &Self::Output {
        &self.0.cursors[index]
    }
}

impl<'a, Idx> IndexMut<Idx> for CursorCollectionMut<'a>
where
    Idx: SliceIndex<[Cursor]>,
{
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        &mut self.0.cursors[index]
    }
}

impl<'a> Drop for CursorCollectionMut<'a> {
    fn drop(&mut self) {
        self.0.sort_and_merge();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_cursor() {
        let mut cursors = CursorCollection::new();
        assert_eq!(1, cursors[..].len());
        cursors.add_cursor(*cursors.main_cursor());
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(0, 0), cursor.position);
        assert_eq!(BufferPosition::line_col(0, 0), cursor.anchor);
        assert!(cursors.next().is_none());

        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(2, 3);
        cursors_mut[0].position = cursors_mut[0].anchor;
        drop(cursors_mut);
        assert_eq!(1, cursors[..].len());
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 2),
            position: BufferPosition::line_col(2, 4),
        });
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(2, 2), cursor.anchor);
        assert_eq!(BufferPosition::line_col(2, 4), cursor.position);
        assert!(cursors.next().is_none());

        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(2, 2);
        cursors_mut[0].position = BufferPosition::line_col(2, 4);
        drop(cursors_mut);
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 2),
            position: BufferPosition::line_col(2, 2),
        });
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(2, 2), cursor.anchor);
        assert_eq!(BufferPosition::line_col(2, 4), cursor.position);
        assert!(cursors.next().is_none());

        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(2, 2);
        cursors_mut[0].position = BufferPosition::line_col(2, 3);
        drop(cursors_mut);
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 4),
            position: BufferPosition::line_col(2, 3),
        });
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(2, 2), cursor.anchor);
        assert_eq!(BufferPosition::line_col(2, 4), cursor.position);
        assert!(cursors.next().is_none());

        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(2, 4);
        cursors_mut[0].position = BufferPosition::line_col(2, 3);
        drop(cursors_mut);
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 3),
            position: BufferPosition::line_col(2, 2),
        });
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(2, 4), cursor.anchor);
        assert_eq!(BufferPosition::line_col(2, 2), cursor.position);
        assert!(cursors.next().is_none());
    }

    #[test]
    fn no_merge_cursor() {
        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(1, 0);
        cursors_mut[0].position = BufferPosition::line_col(1, 0);
        drop(cursors_mut);
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 0),
            position: BufferPosition::line_col(2, 0),
        });
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(1, 0), cursor.anchor);
        assert_eq!(BufferPosition::line_col(1, 0), cursor.position);
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(2, 0), cursor.anchor);
        assert_eq!(BufferPosition::line_col(2, 0), cursor.position);
        assert!(cursors.next().is_none());

        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(3, 2);
        cursors_mut[0].position = BufferPosition::line_col(3, 2);
        drop(cursors_mut);
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 2),
            position: BufferPosition::line_col(2, 2),
        });
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(2, 2), cursor.anchor);
        assert_eq!(BufferPosition::line_col(2, 2), cursor.position);
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(3, 2), cursor.anchor);
        assert_eq!(BufferPosition::line_col(3, 2), cursor.position);
        assert!(cursors.next().is_none());
    }

    #[test]
    fn move_and_merge_cursors() {
        let mut cursors = CursorCollection::new();
        let mut cursors_mut = cursors.as_mut();
        cursors_mut[0].anchor = BufferPosition::line_col(0, 0);
        cursors_mut[0].position = BufferPosition::line_col(0, 0);
        drop(cursors_mut);
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(1, 0),
            position: BufferPosition::line_col(1, 0),
        });
        cursors.add_cursor(Cursor {
            anchor: BufferPosition::line_col(2, 0),
            position: BufferPosition::line_col(2, 0),
        });
        assert_eq!(3, cursors[..].len());
        let mut cursors_mut = cursors.as_mut();
        for c in &mut cursors_mut[..] {
            if c.position.line_index > 0 {
                c.position.line_index -= 1;
            }
            c.anchor = c.position;
        }
        drop(cursors_mut);
        let cursor = cursors.main_cursor();
        assert_eq!(BufferPosition::line_col(1, 0), cursor.anchor);
        assert_eq!(BufferPosition::line_col(1, 0), cursor.position);
        let mut cursors = cursors[..].iter();
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(0, 0), cursor.anchor);
        assert_eq!(BufferPosition::line_col(0, 0), cursor.position);
        let cursor = cursors.next().unwrap();
        assert_eq!(BufferPosition::line_col(1, 0), cursor.anchor);
        assert_eq!(BufferPosition::line_col(1, 0), cursor.position);
        assert!(cursors.next().is_none());
    }
}
