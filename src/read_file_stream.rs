use std::io::Read;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

use actix_web::web::Bytes;
use futures::Stream;

const CHUNK_SIZE: u64 = 1024 * 1024 * 50; // 50 MB

pub struct ReadFileStream {
    file_size: u64,
    processed: usize,
    file: std::fs::File,
    error: bool,
}

impl ReadFileStream {
    pub fn new(path: &Path) -> std::io::Result<Self> {
        Ok(Self {
            file_size: path.metadata()?.len(),
            processed: 0,
            file: std::fs::File::open(path)?,
            error: false,
        })
    }
}

impl Stream for ReadFileStream {
    type Item = actix_web::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.error {
            return Poll::Ready(None);
        }

        let mut chunk = Vec::with_capacity(CHUNK_SIZE as usize);
        let size = self.file.by_ref().take(CHUNK_SIZE).read_to_end(&mut chunk);

        let size = match size {
            Err(e) => {
                eprintln!("Failed to read from file! {}", e);
                self.error = true;
                return Poll::Ready(Some(Err(actix_web::Error::from(e))));
            }
            Ok(size) => size,
        };

        if size == 0 {
            return Poll::Ready(None);
        }

        self.processed += size;

        Poll::Ready(Some(Ok(Bytes::from(chunk))))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.file_size as usize - self.processed, None)
    }
}
