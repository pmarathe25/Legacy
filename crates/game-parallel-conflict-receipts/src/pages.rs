//! Copy-on-write pages retain old receipt generations without full-column clones.
use std::{ops::Index, sync::Arc};
const PAGE: usize = 64;
#[derive(Clone)]
pub struct Pages<T: Clone> {
    pages: Arc<Vec<Arc<Vec<T>>>>,
}
impl<T: Clone> Pages<T> {
    pub fn new(values: Vec<T>) -> Self {
        Self {
            pages: Arc::new(
                values
                    .chunks(PAGE)
                    .map(|page| Arc::new(page.to_vec()))
                    .collect(),
            ),
        }
    }
    pub fn set(&mut self, index: usize, value: T) {
        Arc::make_mut(&mut Arc::make_mut(&mut self.pages)[index / PAGE])[index % PAGE] = value;
    }
}
impl<T: Clone> Index<usize> for Pages<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        &self.pages[index / PAGE][index % PAGE]
    }
}
