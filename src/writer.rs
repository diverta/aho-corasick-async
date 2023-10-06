use std::{collections::VecDeque, task::Poll};
use futures::AsyncWrite;
use pin_project_lite::pin_project;
use crate::AhoCorasick;

// Wrapper over an AsyncWrite. Writing to AhoCorasickAsyncWriter will write replaced results to the underlying writer
pin_project! {
    pub struct AhoCorasickAsyncWriter<'a, W> {
        #[pin]
        sink: W,
        ac: &'a mut AhoCorasick,
        buffer: Vec<u8>, // Used to buffer initially read bytes (before replacements)
        potential_buffer: VecDeque<u8>, // Buffer holding the start of a potential match
        pending_write_buffer: VecDeque<u8>, // Buffer holding the data ready to be written. Might need to wait until next chunk
    }
}

impl<'a, W: AsyncWrite> AhoCorasickAsyncWriter<'a, W> {
    pub fn new(ac: &'a mut AhoCorasick, sink: W) -> Self {
        AhoCorasickAsyncWriter {
            sink,
            ac,
            buffer: Vec::new(),
            potential_buffer: VecDeque::new(),
            pending_write_buffer: VecDeque::new(),
        }
    }
}

impl<'a, W> AsyncWrite for AhoCorasickAsyncWriter<'a, W>
where
    W: AsyncWrite
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        todo!("AsyncWriter not yet implemented")
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        // TODO : Flush pending buffer
        self.project().sink.poll_flush(cx)
    }

    fn poll_close(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        self.project().sink.poll_close(cx)
    }
}