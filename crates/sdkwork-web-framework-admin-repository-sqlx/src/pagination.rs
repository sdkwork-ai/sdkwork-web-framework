//! Repository-level pagination results (`PAGINATION_SPEC.md`).

/// Offset-mode SQL list window with authoritative total count.
#[derive(Debug, Clone)]
pub struct RepoOffsetPage<T> {
    pub items: Vec<T>,
    pub total_items: i64,
}

/// Keyset-mode list window (`LIMIT page_size + 1` already applied at SQL layer).
#[derive(Debug, Clone)]
pub struct RepoKeysetPage<T> {
    pub items: Vec<T>,
    pub has_more: bool,
}

impl<T> RepoKeysetPage<T> {
    pub fn from_limit_plus_one(mut items: Vec<T>, page_size: usize) -> Self {
        let has_more = items.len() > page_size;
        if has_more {
            items.truncate(page_size);
        }
        Self { items, has_more }
    }

    pub fn next_cursor_from_last_id(&self) -> Option<String>
    where
        T: KeysetId,
    {
        if !self.has_more {
            return None;
        }
        self.items
            .last()
            .map(KeysetId::keyset_id)
            .map(|id| id.to_string())
    }
}

pub trait KeysetId {
    fn keyset_id(&self) -> i64;
}
