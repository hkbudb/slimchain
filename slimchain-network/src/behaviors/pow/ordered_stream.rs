use futures::{prelude::*, ready, stream::Fuse};
use pin_project::pin_project;
use slimchain_common::collections::HashMap;
use std::{
    cmp::Ordering,
    fmt::Debug,
    hash::Hash,
    pin::Pin,
    task::{Context, Poll},
};

#[pin_project]
pub struct OrderedStream<S, K, V>
where
    S: Stream<Item = (K, V)>,
{
    #[pin]
    stream: Fuse<S>,
    current: K,
    cache: HashMap<K, V>,
    next_key_fn: Box<dyn Fn(&K) -> K + Sync + Send + 'static>,
}

impl<S, K, V> OrderedStream<S, K, V>
where
    S: Stream<Item = (K, V)>,
{
    pub fn new(
        stream: S,
        current: K,
        next_key_fn: impl Fn(&K) -> K + Sync + Send + 'static,
    ) -> Self {
        Self {
            stream: stream.fuse(),
            current,
            cache: HashMap::new(),
            next_key_fn: Box::new(next_key_fn),
        }
    }
}

impl<S, K, V> Stream for OrderedStream<S, K, V>
where
    K: Eq + Ord + Hash + Debug,
    S: Stream<Item = (K, V)>,
{
    type Item = V;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            if let Some(value) = this.cache.remove(&this.current) {
                *this.current = (this.next_key_fn)(this.current);
                return Poll::Ready(Some(value));
            }

            let item = ready!(this.stream.as_mut().poll_next(cx));

            if let Some((key, value)) = item {
                match key.cmp(this.current) {
                    Ordering::Less => {
                        warn!(
                            "Received outdated item. Got {:?}. Expect {:?}.",
                            key, this.current
                        );
                    }
                    Ordering::Equal => {
                        *this.current = (this.next_key_fn)(this.current);
                        return Poll::Ready(Some(value));
                    }
                    Ordering::Greater => {
                        this.cache.insert(key, value);
                    }
                }
            } else {
                return Poll::Ready(None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;
    use tokio::time::{delay_for, Duration};

    #[tokio::test]
    async fn test_ordered_stream() {
        let (mut tx, rx) = mpsc::unbounded::<(i32, i32)>();
        let mut stream = OrderedStream::new(rx, 0, |x: &i32| x + 1);

        let handle = tokio::spawn(async move {
            for i in 0..=5 {
                assert_eq!(Some(i), stream.next().await);
            }
            assert_eq!(None, stream.next().await);
        });

        tx.send((2, 2)).await.unwrap();
        delay_for(Duration::from_millis(100)).await;
        tx.send((1, 1)).await.unwrap();
        delay_for(Duration::from_millis(100)).await;
        tx.send((0, 0)).await.unwrap();
        delay_for(Duration::from_millis(100)).await;
        tx.send((3, 3)).await.unwrap();
        delay_for(Duration::from_millis(100)).await;
        tx.send((4, 4)).await.unwrap();
        delay_for(Duration::from_millis(100)).await;
        tx.send((5, 5)).await.unwrap();
        delay_for(Duration::from_millis(100)).await;
        tx.close_channel();

        handle.await.unwrap();
    }
}
