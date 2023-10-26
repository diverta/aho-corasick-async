use automaton::AcAutomaton;
use futures::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use reader::AhoCorasickAsyncReader;
use writer::AhoCorasickAsyncWriter;

mod automaton;
mod reader;
mod writer;

#[derive(Debug, Clone)]
pub struct AhoCorasick {
    pub automaton: AcAutomaton,
}

impl AhoCorasick {
    /// Instantiation of the automaton. After instantiation, it is currently not possible to add any new replacements
    /// The constructor argument is a tuple with the searched word as the first element, and an optional replacement as second
    /// Currently the only purpose is performing replacements, so there is little point in having None.
    /// Note that even if None is set, after the word is matched, the state is reset back to root
    pub fn new(replacements: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> Self {
        let ac: AcAutomaton = AcAutomaton::new(
            replacements
        );
        Self {
            automaton: ac,
        }
    }

    /// Obtain AhoCorasickAsyncReader wrapping the original source. Reading from this new reader will yield output with replaced data
    pub fn into_reader<R: AsyncRead>(self, source: R) -> AhoCorasickAsyncReader<R> {
        AhoCorasickAsyncReader::new(self, source)
    }

    /// Obtain AhoCorasickAsyncWriter wrapping the original sink. Writing to this new writer will perform the replacements before sending the bytes to your sink
    pub fn into_writer<W: AsyncWrite>(self, sink: W) -> AhoCorasickAsyncWriter<W> {
        AhoCorasickAsyncWriter::new(self, sink)
    }

    /// Read all data from the reader, perform the replacements, and write to the writer
    /// It is implemented using AhoCorasickAsyncWriter, but either works
    pub async fn try_stream_replace_all<R, W>(self, reader: R, writer: W, buffer_size: usize) -> Result<(), std::io::Error>
    where 
        R: AsyncRead,
        W: AsyncWrite
    {
        let mut buffer = vec![b'\0'; buffer_size];
        let ac_writer = self.into_writer(writer);

        let mut pinned_reader = Box::pin(reader);
        let mut pinned_writer = Box::pin(ac_writer);
        loop {
            let bytes_read = pinned_reader.read(&mut buffer).await?;
            if bytes_read == 0 {
                pinned_writer.close().await?;
                break;
            } else {
                pinned_writer.write(&buffer[..bytes_read]).await?;
            }
        }
        Ok(())
    }
}
