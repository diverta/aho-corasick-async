use std::{collections::VecDeque, task::Poll};
use futures::AsyncRead;
use pin_project_lite::pin_project;
use crate::AhoCorasick;

// Wrapper over an AsyncRead. Reading from AhoCorasickAsyncReader polls replaced results
pin_project! {
    pub struct AhoCorasickAsyncReader<R> {
        #[pin]
        source: R,
        ac: AhoCorasick,
        buffer: Vec<u8>, // Used to buffer initially read bytes (before replacements)
        potential_buffer: VecDeque<u8>, // Buffer holding the start of a potential match
        pending_write_buffer: VecDeque<u8>, // Buffer holding the data ready to be written. Might need to wait until next chunk
    }
}

impl<R: AsyncRead> AhoCorasickAsyncReader<R> {
    pub fn new(ac: AhoCorasick, source: R) -> Self {
        AhoCorasickAsyncReader {
            source,
            ac,
            buffer: Vec::new(),
            potential_buffer: VecDeque::new(),
            pending_write_buffer: VecDeque::new(),
        }
    }
}

impl<R> AsyncRead for AhoCorasickAsyncReader<R>
where
    R: AsyncRead
{
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let this = self.project();
        if this.buffer.len() < buf.len() {
            this.buffer.resize(buf.len(), b'0');
        }
        let mut write_idx: usize = 0;
        while this.pending_write_buffer.len() > 0 {
            // First, write pending buffer if any
            if write_idx < buf.len() {
                buf[write_idx] = this.pending_write_buffer.pop_front().unwrap();
                write_idx += 1;
            } else {
                break;
            }
        }
        if write_idx >= buf.len() {
            // Pending buffer had enough data to fully fill buf => no need to poll from source, wait for next read
            return Poll::Ready(Ok(write_idx));
        }
        match this.source.poll_read(cx, this.buffer) {
            Poll::Ready(result) => {
                match result {
                    Ok(size) => {
                        if size == 0 {
                            // End reached - discard potential buffer
                            while this.potential_buffer.len() > 0 {
                                if write_idx < buf.len() {
                                    buf[write_idx] = this.potential_buffer.pop_front().unwrap();
                                    write_idx += 1;
                                } else {
                                    this.pending_write_buffer.push_back(this.potential_buffer.pop_front().unwrap());
                                }
                            }
                        }
                        for byte in &this.buffer[..size] {
                            this.ac.automaton.next_state(byte);
                            let current_state_depth = this.ac.automaton.state_depth();
                            if this.ac.automaton.is_state_root() {
                                // No potential replacements
                                while this.potential_buffer.len() > 0 {
                                    // At this point potential buffer is discareded (written)
                                    if write_idx < buf.len() {
                                        buf[write_idx] = this.potential_buffer.pop_front().unwrap();
                                        write_idx += 1;
                                    } else {
                                        this.pending_write_buffer.push_back(this.potential_buffer.pop_front().unwrap());
                                    }
                                }
                                if write_idx < buf.len() {
                                    buf[write_idx] = *byte;
                                    write_idx += 1;
                                } else {
                                    this.pending_write_buffer.push_back(*byte);
                                }
                            } else {
                                this.potential_buffer.push_back(*byte);
                                // Either we followed a potential word, or we jumped to a different branch following the suffix link
                                // In the second case, we need to discard (write away) first part of the potential buffer,
                                // keeping as new potential the last part containing the amount of bytes equal to the new state node depth
                                while this.potential_buffer.len() > current_state_depth {
                                    // If current potential word's depth is inferior to the potential buffer, we know that buffer prefix can be discarded
                                    if write_idx < buf.len() {
                                        buf[write_idx] = this.potential_buffer.pop_front().unwrap();
                                        write_idx += 1;
                                    } else {
                                        this.pending_write_buffer.push_back(this.potential_buffer.pop_front().unwrap());
                                    }
                                }
                                if this.ac.automaton.is_state_word() {
                                    // Minimal size word detected => replacement. Currently, the only mode is "first found first replaced", even in case a larger overlapping replacement would've been possible
                                    let from_word: Vec<u8> = this.potential_buffer.drain(..).collect();
                                    if let Some(pos) = this.ac.replace_from.iter().position(|word| &from_word == word) {
                                        // Unless logic error, this should be guaranteed by the nature of AC
                                        let replacement = this.ac.replace_to.get(pos).unwrap(); // Because they were unzipped from tuples, both vec size will always match
                                        for replaced_byte in replacement.into_iter() {
                                            if write_idx < buf.len() {
                                                buf[write_idx] = *replaced_byte;
                                                write_idx += 1;
                                            } else {
                                                this.pending_write_buffer.push_back(*replaced_byte);
                                            }
                                        }
                                        this.ac.automaton.reset_state();
                                    }
                                }
                            }
                        }
                        if write_idx > 0 {
                            // Something has been written
                            Poll::Ready(Ok(write_idx))
                        } else if this.potential_buffer.len() > 0 {
                            // Nothing written, but potential buffer is not empty - request immediate poll again with new buffer
                            // This case happens when the potential buffer (replacement word length) exceeds the current chunk size while matching the entire chunk :
                            // nothing can be written yet, but next chunk(s) are needed to determine the outcome (discard as-is, or replace)
                            cx.waker().clone().wake();
                            Poll::Pending
                        } else {
                            // Nothing left to write
                            Poll::Ready(Ok(0))
                        }
                    },
                    Err(err) => {
                        Poll::Ready(Err(err))
                    }
                }
            },
            Poll::Pending => {
                if write_idx > 0 {
                    // While waiting for the source, if some bytes have already been written from pending buffer, we can return them immediately to speed things up
                    Poll::Ready(Ok(write_idx))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}