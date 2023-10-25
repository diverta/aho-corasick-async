use std::{collections::VecDeque, task::Poll};
use futures::AsyncWrite;
use pin_project_lite::pin_project;
use crate::AhoCorasick;

// Wrapper over an AsyncWrite. Writing to AhoCorasickAsyncWriter will write replaced results to the underlying writer
pin_project! {
    pub struct AhoCorasickAsyncWriter<W> {
        #[pin]
        sink: W,
        ac: AhoCorasick,
        buffer: Vec<u8>, // Buffer holding the data that will be sent to the sink
        potential_buffer: VecDeque<u8>, // Buffer holding the start of a potential match
        pending_state: Option<PendingState> // If the underlying sink responded with Pending, we save the state
    }
}

struct PendingState {
    bytes_to_write: usize, // How much bytes to send from the buffer to the sink
    bytes_read: usize // How much input bytes have been processed (previous buf.len basically)
}

impl<W: AsyncWrite> AhoCorasickAsyncWriter<W> {
    pub fn new(ac: AhoCorasick, sink: W) -> Self {
        AhoCorasickAsyncWriter {
            sink,
            ac,
            buffer: Vec::new(),
            potential_buffer: VecDeque::new(),
            pending_state: None
        }
    }
}

impl<W: AsyncWrite> AhoCorasickAsyncWriter<W> {
    /// Writing to the buffer while making rare incremental resizes
    #[inline(always)]
    fn write_to_buffer(buf: &mut Vec<u8>, idx: &mut usize, char: u8) {
        if *idx >= buf.len() {
            // Since this function is called with incremental idx, we simply double current buffer length every time
            buf.resize(buf.len()*2, b'\0');
        }
        buf[*idx] = char;
        *idx += 1;
    }
}

impl<W> AsyncWrite for AhoCorasickAsyncWriter<W>
where
    W: AsyncWrite
{
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.project();
        if let Some(pending_state) = this.pending_state.take() {
            return match this.sink.poll_write(cx, &this.buffer[..pending_state.bytes_to_write]) {
                Poll::Ready(_) => Poll::Ready(Ok(pending_state.bytes_read)),
                Poll::Pending => {
                    // Still not ready : put back PendingState, as it has been taken
                    *this.pending_state = Some(pending_state);
                    Poll::Pending
                }
            }
        }
        if this.buffer.len() < buf.len() + this.potential_buffer.len() {
            // Default buffer length to buf once to avoid incremental size increases & capacity reallocations during the buffer writing process
            this.buffer.resize(buf.len() + this.potential_buffer.len(), b'\0');
        }
        let mut write_idx = 0usize;
        for byte in buf {
            this.ac.automaton.next_state(byte);
            let current_state_depth = this.ac.automaton.state_depth();
            if this.ac.automaton.is_state_root() {
                // No potential replacements
                while this.potential_buffer.len() > 0 {
                    // At this point potential buffer is discareded (written)
                    Self::write_to_buffer(this.buffer, &mut write_idx, this.potential_buffer.pop_front().unwrap());
                }
                Self::write_to_buffer(this.buffer, &mut write_idx, *byte);
            } else {
                this.potential_buffer.push_back(*byte);
                // Either we followed a potential word, or we jumped to a different branch following the suffix link
                // In the second case, we need to discard (write away) first part of the potential buffer,
                // keeping as new potential the last part containing the amount of bytes equal to the new state node depth
                while this.potential_buffer.len() > current_state_depth {
                    // If current potential word's depth is inferior to the potential buffer, we know that buffer prefix can be discarded
                    Self::write_to_buffer(this.buffer, &mut write_idx, this.potential_buffer.pop_front().unwrap());
                }
                if this.ac.automaton.is_state_word() {
                    // Minimal size word detected => replacement. Currently, the only mode is "first found first replaced", even in case a larger overlapping replacement would've been possible
                    if let Some(replacement) = this.ac.automaton.state_replacement() {
                        // Replacement is given by the automaton node, so we only need to clear the potential buffer
                        this.potential_buffer.clear();
                        for replaced_byte in replacement.iter() {
                            Self::write_to_buffer(this.buffer, &mut write_idx, *replaced_byte);
                        }
                    } else {
                        // We have reached a word, but it has no replacement - with the current constructor this case is not possible
                        // However maybe in the future a search without replace feature might be added, and here's where it can be handled
                        // In the meanwhile, we will simply discard the buffer. The state will be reset in all cases, as if the word had been found
                        while this.potential_buffer.len() > 0 {
                            Self::write_to_buffer(this.buffer, &mut write_idx, this.potential_buffer.pop_front().unwrap());
                        }
                    }
                    this.ac.automaton.reset_state();
                }
            }
        }
        // Now (unless buf was empty), either the bytes are in the buffer ready to be written, or they are in the potential buffer awaiting for the next chunk before being written
        // In both cases, all of them are considered "written" from the standpoint of AhoCorasickAsyncWriter, and we need to return not how many we have actually written to the sink with replacements,
        // but how many we have "consumed" - which should always match the length of input buf. So the return count is independent from write_idx
        if write_idx > 0 {
            match this.sink.poll_write(cx, &this.buffer[..write_idx]) {
                Poll::Ready(_) => Poll::Ready(Ok(buf.len())),
                Poll::Pending => {
                    // Tricky state : the sink is not yet ready to accept the buffer, but we have processed the chunk, including moving automaton state around
                    // So because don't want to redo the processing, we simply save the Pending state with current buffer & write idx,
                    // and on the next call at the beginning of this poll_write, this Pending state is handled
                    *this.pending_state = Some(PendingState {
                        bytes_to_write: write_idx,
                        bytes_read: buf.len()
                    });
                    Poll::Pending
                },
            }
        } else if this.potential_buffer.len() > 0 {
            // Nothing written, but potential buffer is not empty - request immediate poll again with new buffer by saying we have accepted the buffer fully
            // This case happens when the potential buffer (replacement word length) exceeds the current chunk size while matching the entire chunk :
            // nothing can be written yet, but next chunk(s) are needed to determine the outcome (discard as-is, or replace)
            // Different to the Reader, here we cannot reply with Pending, as same bytes will be sent again - we have to acknowledge that we have consumed them
            Poll::Ready(Ok(buf.len()))
        } else {
            // This case can happen in 2 scenarios :
            // 1. Input buf is empty (most likely a bug on the consumer side)
            // 2. The contents of buf match entirely a word which has the empty string replacement. We still inform the consumer that we have "written" the bytes we received,
            //    even though we has nothing to write to the sink
            Poll::Ready(Ok(buf.len()))
        }
    }

    fn poll_flush(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        // Nothing special to do here
        self.project().sink.poll_flush(cx)
    }

    fn poll_close(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.project();
        if this.potential_buffer.len() > 0 {
            // We have to ensure that potential buffer bytes are written, in case there was a beginning of a match at the end of the stream
            this.potential_buffer.make_contiguous();
            match this.sink.poll_write(cx, this.potential_buffer.as_slices().0) {
                Poll::Ready(_) => {
                    // Bytes have been written : empty potential_buffer, and ask for the next call to poll_close
                    this.potential_buffer.clear();
                    cx.waker().wake_by_ref();
                    Poll::Pending

                },
                Poll::Pending => Poll::Pending // The last bytes can't be written yet, so poll_close will be called again when sink.poll_write is ready to make progress
            }
        } else {
            this.sink.poll_close(cx)
        }
    }
}