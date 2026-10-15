/// Merge sorted unique streams without materializing or sorting their union.
pub fn merge_unique<T: Ord>(
    left: impl Iterator<Item = T>,
    right: impl Iterator<Item = T>,
) -> impl Iterator<Item = T> {
    let mut left = left.peekable();
    let mut right = right.peekable();
    std::iter::from_fn(move || {
        let ordering = match (left.peek(), right.peek()) {
            (Some(a), Some(b)) => Some(a.cmp(b)),
            (Some(_), None) => Some(std::cmp::Ordering::Less),
            (None, Some(_)) => Some(std::cmp::Ordering::Greater),
            (None, None) => None,
        }?;
        match ordering {
            std::cmp::Ordering::Less => left.next(),
            std::cmp::Ordering::Greater => right.next(),
            std::cmp::Ordering::Equal => {
                right.next();
                left.next()
            }
        }
    })
}
