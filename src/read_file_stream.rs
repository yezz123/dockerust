use std::io::{ErrorKind, Read};
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
    type Item = std::io::Result<Bytes>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.error {
            return Poll::Ready(None);
        }

        let mut chunk = Vec::with_capacity(CHUNK_SIZE as usize);
        let size = self.file.by_ref()
            .take(CHUNK_SIZE)
            .read_to_end(&mut chunk);

        let size = match size {
            Err(e) => {
                eprintln!("Failed to read from file! {}", e);
                self.error = true;
                return Poll::Ready(Some(Err(std::io::Error::new(
                    ErrorKind::Other,
                    "Failed to read data!".to_string(),
                ))));
            }
            Ok(size) => size
        };

        if size == 0 {
            return Poll::Ready(None);
        }

        self.processed += size ;

        Poll::Ready(Some(Ok(Bytes::from(chunk))))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.file_size as usize - self.processed, None)
    }
}
