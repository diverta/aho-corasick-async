use automaton::AcAutomaton;
use futures::AsyncRead;
use reader::AhoCorasickAsyncReader;

mod automaton;
mod reader;
//mod writer;

#[derive(Debug)]
pub struct AhoCorasick {
    pub automaton: AcAutomaton,
    replace_from: Vec<Vec<u8>>,
    replace_to: Vec<Vec<u8>>,
}

impl AhoCorasick {
    /// Instantiate the automaton. After instantiation, it is currently not possible to add any new replacements
    pub fn new(replacements: Vec<(Vec<u8>, Vec<u8>)>) -> Self {
        let (replace_from, replace_to): (Vec<Vec<u8>>, Vec<Vec<u8>>) = replacements.into_iter().unzip();
        let ac: AcAutomaton = AcAutomaton::new(
            replace_from.iter().map(|v| v.as_ref()).collect()
        );
        Self {
            automaton: ac,
            replace_from,
            replace_to,
        }
    }

    /// Obtain AhoCorasickAsyncReader wrapping the original source. Reading from this new reader will yield output with replaced data
    pub fn into_reader<R: AsyncRead>(self, source: R) -> AhoCorasickAsyncReader<R> {
        AhoCorasickAsyncReader::new(self, source)
    }
}