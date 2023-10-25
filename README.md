Simple and safe implementation of AhoCorasick automaton in Async context

## Motivation

In non async context, look no further than the popular [aho-corasick](https://github.com/BurntSushi/aho-corasick) crate.

However, due to the (current) [decision to not support async](https://github.com/BurntSushi/aho-corasick/issues/95) in that crate, at the time of writing there seems to be no alternative when wanting to perform efficient replacement in the async context. Hence writing a simple library for that purpose only. Which is why here we keep the features/interfaces to the minimal necessary, but open to improvements and PRs.

## Instantiating the AhoCorasick automaton

To instantiate an AhoCorasick struct, you need a Vec of tuples having the word as Vec<u8> and its replacement as Option<Vec<u8>> :
```rust
let replacements = Vec::from(
    [
        ("old_word_one".as_bytes().to_vec(), Some("new_word_one".as_bytes().to_vec())),
        ("old_word_two".as_bytes().to_vec(), Some("new_word_two".as_bytes().to_vec())),
    ]
);

let ac: AhoCorasick = AhoCorasick::new(replacements);
```

Currently the only supported strategy is to match the first found word and replace it, resetting the state and continuing from this point forward. This means that :
1. In case of overlapping suffixes, the largest word is prioritized. Example : `she` and `he` => `she` has the priority, because we are already on the longest Trie branch.
2. In case of overlapping prefixes, the smallest word is prioritized. Example : `her` and `he` => `he` has the priority, because when walking down a Trie branch, we replace as soon as we find the first match.

Replacement of replacement is equally not supported. Not having a replacement for a word means that when this word is found, the state is reset, so this word will never be used as a partial match for another replacement. You can use this to "protect" some words from having them replaced as part of bigger overlaps.

AhoCorasick struct does implement cheap-ish Clone, as only pointers to the nodes are cloned. They will point to the same underlying node data, however this data is not mutable after the automaton is built (except for the state pointer of course, which is reset on clone). So if multiple usages are needed, build it once, and clone before converting into additional readers or writers


## Usage

Now, on to the usage. The interface consists of implementing AsyncRead and AsyncWrite traits of `futures` crate :

### Usage 1. Using AhoCorasickAsyncReader, which wraps around your own AsyncRead, and replacements are performed when polling from it.

```rust
let source_reader = ... // Provide any object implementing AsyncRead trait
let mut ac_reader = ac.into_reader(source_reader); // 'ac' has been defined in the first code example

// Now simply read from ac_reader instead of your source_reader

// SAMPLE code demonstrating the usage of performing replacements into a string
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

### Usage 2. Using AhoCorasickAsyncWriter, which wraps around your own AsyncWrite (the sink), and replacements are performed when writing to it

```rust
let sink = ... // Provide any object implementing AsyncWrite trait
let mut ac_writer = ac.into_writer(sink); // 'ac' has been defined in the first code example

// You can now use ac_writer instead of sink
```


### Usage 3. If a simple polling of everything from an AsyncRead source, replacement, and writing all into an AsyncWrite sink is what you desire, a helper method is available :

```rust
let mut reader = ... // Your AsyncRead object
let mut writer = ... // Your AsyncWrite object
let buffer_size = 8096; // Pick a buffer size that fits your case the best

// Do the replacements !
let result: Result<(), std::io::Error> = ac.try_stream_replace_all(&mut reader, &mut writer, test_buffer_size).await;
```

It only requires you to specify the desired buffer size for the reading, to avoid making any assumptions. This helper method body is pretty straightforward and could be easily implemented manually.

## Performance

By its nature, Aho-Corasick algorithm outperforms any manual scans/replacements, and the number of replacements have little to no impact on the performance which is always in linear time with the input size. However, the main focus of this crate is a working and safe implementation in the async context, where the performance bottleneck is often times not CPU-bound processing, but rather waiting to to receive or send the bytes in an async environment.

That being said, let's get a rough idea of how aho-corasick-async performs in a CPU-bound process.

From very simple benchmarks comparatively with the [aho-corasick](https://github.com/BurntSushi/aho-corasick) crate and the basic manual comparison code along the lines of :
```rust
    let input = ... // Some very large String loaded from a file
    let words: Vec<(&str, &str)> = Vec::from([
        ... // various replacement tests
    ]);
    let buffer_size = 64 * (1 << 10); // 64 KB - seems to be the default in aho-corasick (found there)
    let output_size = ... // Make sure the output buffer is correctly pre-sized to avoid reallocations
    
    let t_aho_corasick: Duration = {
        let (patterns, replacements): (Vec<_>, Vec<_>) = words.iter().cloned().unzip();
        let ac = aho_corasick::AhoCorasick::new(patterns).unwrap();
        let t0 = Instant::now();
        ac.try_stream_replace_all(input.as_bytes(), &mut ac_output, &replacements).unwrap();
        t0.elapsed()
    };

    let t_aho_corasick_async: Duration = {
        let replacements: Vec<(Vec<u8>, Option<Vec<u8>>)> = words.iter().cloned()
            .map(|w| (w.0.as_bytes().to_vec(), Some(w.1.as_bytes().to_vec()))).collect();

        let ac = aho_corasick_async::AhoCorasick::new(replacements);

        let reader = futures::io::Cursor::new(input);
        let writer = futures::io::Cursor::new(&mut aca_output);

        let t0 = Instant::now();
        futures::executor::block_on(async {
            ac.try_stream_replace_all(reader, writer, buffer_size).await.unwrap();
        });
        t0.elapsed()
    };
```

Comparing the times, currently standard non-async aho-corasick performs anywhere between 2 to 10 times faster, depending on the number of replacements, matching patterns, etc. Good news is, the ratio of both performances does not change with input size, and is constant.

Aside from futures overhead, the slowness is due to the automaton navigation code which is all but optimal, due to the usage of RefCells and no implementation of advanced techniques such as memory efficient layouts ensuring fast node traversal (yet). As of the 0.1.0, the features are equally minimal and will be added when/if need arises.
