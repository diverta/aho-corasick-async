use std::str::from_utf8;

use aho_corasick_async::AhoCorasick;
use futures::{AsyncReadExt, executor::block_on, AsyncWriteExt};
use test_utils::{BytesAsyncReader, BytesAsyncWriter};

mod test_utils;

#[test]
fn test_reader() {
    block_on(async {
        // Multiple test cases
        for (test_index, (source_string, replacements, expected_output)) in [
            (
                "abc".to_string(),
                Vec::from(
                    [
                        ("ab".as_bytes().to_vec(), Some("AB".as_bytes().to_vec())),
                    ]
                ),
                "ABc".to_string(),
            ),
            (
                // Empty replacement
                "abc".to_string(),
                Vec::from(
                    [
                        ("abc".as_bytes().to_vec(), Some("".as_bytes().to_vec())),
                    ]
                ),
                "".to_string(),
            ),
            (
                "abcdef".repeat(100),
                Vec::from(
                    [
                        ("ab".as_bytes().to_vec(), Some("ABAB".as_bytes().to_vec())),
                        ("efg".as_bytes().to_vec(), Some("EFG".as_bytes().to_vec())),
                    ]
                ),
                "ABABcdef".repeat(100),
            ),
            (
                "a".repeat(10),
                Vec::from(
                    [
                        ("aaaa".as_bytes().to_vec(), Some("AAAA".as_bytes().to_vec())),
                    ]
                ),
                "AAAAAAAAaa".to_owned(),
            ),
            (
                "Now he is here, so is she. his hair is blond, her bag is big".to_owned(),
                Vec::from(
                    [
                        ("he".as_bytes().to_vec(), Some("she".as_bytes().to_vec())),
                        ("she".as_bytes().to_vec(), Some("he".as_bytes().to_vec())),
                        ("his".as_bytes().to_vec(), Some("her".as_bytes().to_vec())),
                        ("her".as_bytes().to_vec(), Some("his".as_bytes().to_vec())), // This replacement should never occur, as 'he' will have priority
                    ]
                ),
                "Now she is shere, so is he. her hair is blond, sher bag is big".to_owned(),
            ),
            (
                // Using the protected words
                "'he' is replaced, but not 'she' nor 'ashe'. 'shed' will also not be replaced as 'she' is prioritized".to_owned(),
                Vec::from(
                    [
                        ("she".as_bytes().to_vec(), None), // "she" is protected, as finding it resets the state
                        ("he".as_bytes().to_vec(), Some("him".as_bytes().to_vec())), // No replacement
                        ("shed".as_bytes().to_vec(), Some("shack".as_bytes().to_vec())), // No replacement
                    ]
                ),
                "'him' is replaced, but not 'she' nor 'ashe'. 'shed' will also not be replaced as 'she' is prioritized".to_owned(),
            ),
        ].iter().enumerate() {
            // Multiple buffer sizes
            for test_buffer_size in [1,2,3,5,10,100] {
                let ac = AhoCorasick::new(replacements.clone());

                let mut buf: Vec<u8> = Vec::with_capacity(test_buffer_size);
                buf.resize(test_buffer_size, 0u8);
                println!("Test #{}, buffer size {} ...", test_index, test_buffer_size);
                {
                    // Testing the Reader : with and without forced_pending
                    for forced_pending in [0usize, 2] {
                        let reader = BytesAsyncReader::new(source_string.as_bytes().to_vec(), forced_pending);
                        let mut ac_reader = ac.clone().into_reader(reader);
    
                        let mut output: Vec<u8> = Vec::new();
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
                        assert_eq!(from_utf8(&output).unwrap_or("<utf8 error>"), expected_output);
                    }
                }
                {
                    // Testing the Writer : with and without forced_pending
                    for forced_pending in [0usize, 2] {
                        let mut reader = BytesAsyncReader::new(source_string.as_bytes().to_vec(), 0);
                        let writer = BytesAsyncWriter::new(forced_pending);
                        let mut ac_writer = ac.clone().into_writer(writer.clone());
    
                        loop {
                            match reader.read(&mut buf).await {
                                Ok(size) => {
                                    if size == 0 {
                                        ac_writer.close().await.unwrap();
                                        break;
                                    } else {
                                        ac_writer.write(&buf[..size]).await.unwrap();
                                    }
                                },
                                Err(err) => {
                                    panic!("BytesAsyncReader error : {}", err)
                                },
                            }
                        }
                        assert_eq!(from_utf8(&writer.sink.borrow()).unwrap_or("<utf8 error>"), expected_output);
                    }
                }
                {
                    for forced_pending in [0usize, 2] {
                        let mut reader = BytesAsyncReader::new(source_string.as_bytes().to_vec(), forced_pending);
                        let mut writer = BytesAsyncWriter::new(forced_pending);
                        
                        let result = ac.clone().try_stream_replace_all(&mut reader, &mut writer, test_buffer_size).await;
                        assert!(result.is_ok());
                        assert_eq!(from_utf8(&writer.sink.borrow()).unwrap_or("<utf8 error>"), expected_output);
                    }
                }
            }
        }
    });
}
