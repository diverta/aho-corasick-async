use std::{task::Poll, str::from_utf8};

use aho_corasick_async::AhoCorasick;
use futures::{AsyncRead, AsyncReadExt, executor::block_on};

struct BytesAsyncReader {
    source: Vec<u8>,
    cursor: usize,
}

impl BytesAsyncReader {
    fn new(source: Vec<u8>) -> Self {
        Self { source, cursor: 0 }
    }
}

impl AsyncRead for BytesAsyncReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        if self.cursor >= self.source.len() {
            return Poll::Ready(Ok(0));
        }
        let source_from_cursor = &self.source[self.cursor..];
        let remaining_len = source_from_cursor.len();
        let buf_len = buf.len();
        if buf_len >= remaining_len {
            buf[..remaining_len].copy_from_slice(source_from_cursor);
            self.cursor += remaining_len;
            Poll::Ready(Ok(remaining_len))
        } else {
            buf[..].copy_from_slice(&source_from_cursor[..buf_len]);
            self.cursor += buf_len;
            Poll::Ready(Ok(buf.len()))
        }
    }
}

#[test]
fn test_reader() {
    block_on(async {
        // Multiple test cases
        for (test_index, (source_string, replacements, expected_output)) in [
            (
                "abcdef".repeat(100),
                Vec::from(
                    [
                        ("ab".as_bytes().to_vec(), "ABAB".as_bytes().to_vec()),
                        ("e".as_bytes().to_vec(), "EE".as_bytes().to_vec()),
                    ]
                ),
                "ABABcdEEf".repeat(100),
            ),
            (
                "Now he is here, so is she. his hair is blond, her bag is big".to_owned(),
                Vec::from(
                    [
                        ("he".as_bytes().to_vec(), "she".as_bytes().to_vec()),
                        ("she".as_bytes().to_vec(), "he".as_bytes().to_vec()),
                        ("his".as_bytes().to_vec(), "her".as_bytes().to_vec()),
                        ("her".as_bytes().to_vec(), "his".as_bytes().to_vec()), // This replacement should never occur, as 'he' will have priority
                    ]
                ),
                "Now she is shere, so is he. her hair is blond, sher bag is big".to_owned(),
            ),
        ].iter().enumerate() {
            // Multiple buffer sizes
            for test_buffer_size in [1,2,3,5,10,100] {
                let reader = BytesAsyncReader::new(source_string.as_bytes().to_vec());
                let ac = AhoCorasick::new(replacements.clone());
                let mut ac_reader = ac.into_reader(reader);

                let mut output: Vec<u8> = Vec::new();
                let mut buf: Vec<u8> = Vec::with_capacity(test_buffer_size);
                buf.resize(test_buffer_size, 0u8);
                loop {
                    match ac_reader.read(&mut buf).await {
                        Ok(size) => {
                            if size == 0 {
                                break;
                            } else {
                                output.extend(&buf[..size]);
                            }
                        },
                        Err(err) => {
                            panic!("BytesAsyncReader error : {}", err)
                        },
                    }
                }
                println!("Test #{}, buffer size {} ...", test_index, test_buffer_size);
                assert_eq!(from_utf8(&output).unwrap_or("<utf8 error>"), expected_output);
            }
        }
    });
}
