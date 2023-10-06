Simple and safe implementation of AhoCorasick automaton in Async context.

The interface currently consists of AhoCorasickAsyncReader, which wraps around your own AsyncReader, and replacements are performed when polling from it.

Example usage :
```
    let source_reader = ... // Provide any object implementing AsyncReader trait
    let replacements = Vec::from(
        [
            ("old_word_one".as_bytes().to_vec(), "new_word_one".as_bytes().to_vec()),
            ("old_word_two".as_bytes().to_vec(), "new_word_two".as_bytes().to_vec()),
        ]
    );

    let ac = AhoCorasick::new(replacements);
    let mut ac_reader = ac.into_reader(source_reader);

    // Now simply read from ac_reader instead of your source_reader

    // Below is an example of buffering to a string
    async {
        let mut output_bytes: Vec<u8> = Vec::new();
        let mut buf = [0 as u8; 100];
        loop {
            match ac_reader.read(&mut buf).await {
                Ok(size) => {
                    if size == 0 {
                        break;
                    } else {
                        output_bytes.extend(&buf[..size]);
                    }
                },
                Err(err) => {
                    panic!("Read error : {}", err)
                },
            }
        }
        let output_string: String = String::from_utf8(output_bytes).unwrap();
    }

```

TODO :

In a similar fashion implement AhoCorasickAsyncWriter as a wrapper over an AsyncWrite