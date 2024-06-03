/// Provides a wrapper around [Vec::push] which returns the Vec when its length reaches `batch_size`.
pub(crate) struct Batcher<T> {
    pub batch: Vec<T>,
    pub batch_size: usize,
}

impl<T> Batcher<T> {
    pub fn new(batch_size: usize) -> Self {
        Self {
            batch: Vec::with_capacity(batch_size),
            batch_size,
        }
    }

    pub fn push(mut self, x: T) -> (Self, Option<Vec<T>>) {
        self.batch.push(x);
        if self.batch.len() >= self.batch_size {
            (Self::new(self.batch_size), Some(self.batch))
        } else {
            (self, None)
        }
    }

    pub fn into_inner(self) -> Vec<T> {
        self.batch
    }
}

// Note: implementing Drop annoyingly causes E0509
// cannot move out of type `Batcher<T>`, which implements the `Drop` trait
//
// impl<T> Drop for Batcher<T> {
//     fn drop(&mut self) {
//         if !self.batch.is_empty() {
//             tracing::warn!(
//                 "Batcher<{}> dropped but it was not empty. {} objects ignored."
//                 std::any::type_name::<T>(),
//                 self.batch.len()
//             )
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batcher() {
        let batches0 = Batcher::new(3);
        let (batches1, r0) = batches0.push("ChRIS");
        assert_eq!(r0, None);
        let (batches2, r1) = batches1.push("is");
        assert_eq!(r1, None);
        let (batches3, r2) = batches2.push("an");
        assert_eq!(r2, Some(vec!["ChRIS", "is", "an"]));
        let (batches4, r3) = batches3.push("open-source");
        assert_eq!(r3, None);
        let (batches5, r4) = batches4.push("software");
        assert_eq!(r4, None);
        assert_eq!(batches5.into_inner(), vec!["open-source", "software"])
    }
}
