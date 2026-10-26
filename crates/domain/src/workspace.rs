//! The open workspace: the item list and the settings. Nothing here is saved.

use crate::items::{ImportedSource, Item, ItemId, SourceId};
use crate::settings::JobSettings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceError {
    UnknownItem(ItemId),
    NotAPermutation,
    NotAPack(ItemId),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorkspaceError::UnknownItem(id) => write!(f, "no item with id {}", id.0),
            WorkspaceError::NotAPermutation => write!(f, "new order must contain every item exactly once"),
            WorkspaceError::NotAPack(id) => write!(f, "item {} has only one page", id.0),
        }
    }
}

impl std::error::Error for WorkspaceError {}

#[derive(Clone, Debug, Default)]
pub struct Workspace {
    items: Vec<Item>,
    next_item: u64,
    pub settings: JobSettings,
}

impl Workspace {
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn references(&self, source: SourceId) -> bool {
        self.items.iter().any(|item| item.source == source)
    }

    pub fn add(&mut self, source: ImportedSource) -> ItemId {
        let id = self.allocate();
        self.items.push(Item {
            id,
            source: source.id,
            name: source.name,
            format: source.format,
            pages: source.pages,
            copies: None,
        });
        id
    }

    pub fn remove(&mut self, id: ItemId) -> Result<Item, WorkspaceError> {
        let index = self.index_of(id)?;
        Ok(self.items.remove(index))
    }

    pub fn clear(&mut self) -> Vec<Item> {
        std::mem::take(&mut self.items)
    }

    pub fn reorder(&mut self, order: &[ItemId]) -> Result<(), WorkspaceError> {
        if order.len() != self.items.len() {
            return Err(WorkspaceError::NotAPermutation);
        }
        let mut reordered = Vec::with_capacity(order.len());
        for id in order {
            let item = self.item(*id).ok_or(WorkspaceError::NotAPermutation)?;
            if reordered.iter().any(|existing: &Item| existing.id == *id) {
                return Err(WorkspaceError::NotAPermutation);
            }
            reordered.push(item.clone());
        }
        self.items = reordered;
        Ok(())
    }

    /// Replace a pack with one item per page, in place, each keeping the pack's copy count.
    pub fn split(&mut self, id: ItemId) -> Result<Vec<ItemId>, WorkspaceError> {
        let index = self.index_of(id)?;
        if !self.items[index].is_pack() {
            return Err(WorkspaceError::NotAPack(id));
        }
        let pack = self.items.remove(index);
        let mut ids = Vec::with_capacity(pack.pages.len());
        for (offset, page) in pack.pages.iter().enumerate() {
            let page_id = self.allocate();
            ids.push(page_id);
            self.items.insert(
                index + offset,
                Item {
                    id: page_id,
                    source: pack.source,
                    name: pack.name.clone(),
                    format: pack.format,
                    pages: vec![*page],
                    copies: pack.copies,
                },
            );
        }
        Ok(ids)
    }

    /// `None` (or zero) returns the item to auto.
    pub fn set_copies(&mut self, id: ItemId, copies: Option<u32>) -> Result<(), WorkspaceError> {
        let index = self.index_of(id)?;
        self.items[index].copies = copies.filter(|&c| c > 0);
        Ok(())
    }

    fn index_of(&self, id: ItemId) -> Result<usize, WorkspaceError> {
        self.items.iter().position(|item| item.id == id).ok_or(WorkspaceError::UnknownItem(id))
    }

    fn allocate(&mut self) -> ItemId {
        self.next_item += 1;
        ItemId(self.next_item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::items::{ArtKind, PageArt, SourceFormat};

    fn source(id: u64, pages: usize) -> ImportedSource {
        ImportedSource {
            id: SourceId(id),
            name: format!("file-{id}"),
            format: if pages > 1 { SourceFormat::Pdf } else { SourceFormat::Jpeg },
            pages: (0..pages)
                .map(|page| PageArt { page, size: Size::new(360.0, 504.0), kind: ArtKind::Vector })
                .collect(),
        }
    }

    // I-06, I-07
    #[test]
    fn splitting_a_pack_keeps_position_and_copies() {
        let mut ws = Workspace::default();
        let a = ws.add(source(1, 1));
        let pack = ws.add(source(2, 3));
        let c = ws.add(source(3, 1));
        ws.set_copies(pack, Some(6)).unwrap();
        let pages = ws.split(pack).unwrap();
        let ids: Vec<_> = ws.items().iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![a, pages[0], pages[1], pages[2], c]);
        assert!(ws.items()[1..4].iter().all(|i| i.copies == Some(6) && !i.is_pack()));
        assert_eq!(ws.items()[2].pages[0].page, 1);

        // An image can now sit between pages.
        ws.reorder(&[pages[0], c, pages[1], pages[2], a]).unwrap();
        assert_eq!(ws.items()[1].id, c);
        assert_eq!(ws.split(a), Err(WorkspaceError::NotAPack(a)));
    }

    // I-02, I-03, I-04, I-05
    #[test]
    fn list_operations() {
        let mut ws = Workspace::default();
        let a = ws.add(source(1, 1));
        let b = ws.add(source(2, 1));
        ws.set_copies(a, Some(0)).unwrap();
        assert_eq!(ws.item(a).unwrap().copies, None, "zero reverts to auto");
        ws.set_copies(a, Some(3)).unwrap();
        ws.set_copies(a, None).unwrap();
        assert_eq!(ws.item(a).unwrap().copies, None, "clearing reverts to auto");
        assert_eq!(ws.reorder(&[a]), Err(WorkspaceError::NotAPermutation));
        assert_eq!(ws.reorder(&[a, a]), Err(WorkspaceError::NotAPermutation));
        ws.reorder(&[b, a]).unwrap();
        assert_eq!(ws.items()[0].id, b);
        ws.remove(b).unwrap();
        assert!(!ws.references(SourceId(2)));
        assert_eq!(ws.clear().len(), 1);
        assert!(ws.items().is_empty());
    }
}
