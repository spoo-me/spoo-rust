//! Pagination primitives.

use crate::error::Error;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type BoxFuture<T> = std::pin::Pin<Box<dyn Future<Output = T> + Send>>;
#[cfg(target_arch = "wasm32")]
pub(crate) type BoxFuture<T> = std::pin::Pin<Box<dyn Future<Output = T>>>;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) type PageFetcher<T> = Box<dyn Fn() -> BoxFuture<Result<Page<T>, Error>> + Send + Sync>;
#[cfg(target_arch = "wasm32")]
pub(crate) type PageFetcher<T> = Box<dyn Fn() -> BoxFuture<Result<Page<T>, Error>>>;

/// One page of a listing, with the cursor to the next.
///
/// `items` is the page's data; the pagination facts mirror the wire. Walk
/// manually with [`Page::next_page`], or lazily with [`Page::stream`]
/// (`stream` feature) which yields items across page boundaries and fetches
/// each page only when the previous one is exhausted.
pub struct Page<T> {
    /// The items on this page.
    pub items: Vec<T>,
    /// 1-based page number.
    pub page: u64,
    /// Page size the server applied.
    pub page_size: u64,
    /// Total items across all pages.
    pub total: u64,
    /// Whether another page exists.
    pub has_next: bool,
    pub(crate) next: Option<PageFetcher<T>>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for Page<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("items", &self.items)
            .field("page", &self.page)
            .field("page_size", &self.page_size)
            .field("total", &self.total)
            .field("has_next", &self.has_next)
            .finish_non_exhaustive()
    }
}

impl<T> Page<T> {
    /// Fetch the next page, or `None` when this was the last one.
    pub async fn next_page(&self) -> Result<Option<Page<T>>, Error> {
        match (&self.next, self.has_next) {
            (Some(fetch), true) => fetch().await.map(Some),
            _ => Ok(None),
        }
    }
}

#[cfg(feature = "stream")]
mod stream {
    use super::*;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// A lazy item stream over every page of a listing. Created by
    /// [`Page::stream`].
    pub struct PageStream<T> {
        buffer: std::collections::VecDeque<T>,
        next: Option<PageFetcher<T>>,
        has_next: bool,
        pending: Option<BoxFuture<Result<Page<T>, Error>>>,
    }

    impl<T> Page<T> {
        /// Consume this page into a stream of items that fetches following
        /// pages on demand.
        pub fn stream(self) -> PageStream<T> {
            PageStream {
                buffer: self.items.into(),
                next: self.next,
                has_next: self.has_next,
                pending: None,
            }
        }
    }

    impl<T> futures_core::Stream for PageStream<T> {
        type Item = Result<T, Error>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            // Fields are all Unpin; the future is re-pinned on poll.
            let this = self.get_mut();
            loop {
                if let Some(item) = this.buffer.pop_front() {
                    return Poll::Ready(Some(Ok(item)));
                }
                if let Some(pending) = this.pending.as_mut() {
                    match pending.as_mut().poll(cx) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Ok(page)) => {
                            this.pending = None;
                            this.buffer = page.items.into();
                            this.has_next = page.has_next;
                            this.next = page.next;
                            continue;
                        }
                        Poll::Ready(Err(err)) => {
                            this.pending = None;
                            this.has_next = false;
                            this.next = None;
                            return Poll::Ready(Some(Err(err)));
                        }
                    }
                }
                match (&this.next, this.has_next) {
                    (Some(fetch), true) => {
                        this.pending = Some(fetch());
                    }
                    _ => return Poll::Ready(None),
                }
            }
        }
    }

    impl<T> Unpin for PageStream<T> {}
}

#[cfg(feature = "stream")]
pub use stream::PageStream;
