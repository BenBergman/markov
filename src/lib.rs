//! A generic [Markov chain](https://en.wikipedia.org/wiki/Markov_chain) for almost any type. This 
//! uses HashMaps internally, and so Eq and Hash are both required.
//!
//! # Examples
//!
//! ```
//! use markov::Chain;
//! 
//! let mut chain = Chain::new();
//! chain.feed_str("I like cats and I like dogs.");
//! println!("{}", chain.generate_str());
//! ```
//!
//! ```
//! use markov::Chain;
//!
//! let mut chain = Chain::new();
//! chain.feed(vec![1u8, 2, 3, 5]).feed(vec![3u8, 9, 2]);
//! println!("{}", chain.generate());
//! ```
#![experimental]
#![feature(slicing_syntax)]
#![warn(missing_docs)]

extern crate "rustc-serialize" as rustc_serialize;

use std::borrow::ToOwned;
use std::collections::HashMap;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::hash::Hash;
use std::io::{BufferedReader, File, InvalidInput, IoError, IoResult};
use std::iter::Map;
use std::rand::{Rng, task_rng};
use std::rc::Rc;
use rustc_serialize::{Decodable, Encodable};
use rustc_serialize::json::{Decoder, DecoderError, Encoder, decode, encode};

/// A generic [Markov chain](https://en.wikipedia.org/wiki/Markov_chain) for almost any type. This 
/// uses HashMaps internally, and so Eq and Hash are both required.
#[deriving(RustcEncodable, RustcDecodable, PartialEq, Show)]
pub struct Chain<T: Eq + Hash> {
    map: HashMap<Option<Rc<T>>, HashMap<Option<Rc<T>>, uint>>,
}

impl<T: Eq + Hash> Chain<T> {
    /// Constructs a new Markov chain. 
    pub fn new() -> Chain<T> {
        Chain {
            map: {
                let mut map = HashMap::new();
                map.insert(None, HashMap::new());
                map
            }
        }
    }

    /// Determines whether or not the chain is empty. A chain is considered empty if nothing has
    /// been fed into it.
    pub fn is_empty(&self) -> bool {
        let start: Option<Rc<T>> = None;
        self.map[start].is_empty()
    }


    /// Feeds the chain a collection of tokens. This operation is O(n) where n is the number of
    /// tokens to be fed into the chain.
    pub fn feed(&mut self, tokens: Vec<T>) -> &mut Chain<T> {
        if tokens.len() == 0 { return self }
        let mut toks = Vec::new();
        toks.push(None);
        toks.extend(tokens.into_iter().map(|token| {
            let rc = Rc::new(token);
            if !self.map.contains_key(&Some(rc.clone())) {
                self.map.insert(Some(rc.clone()), HashMap::new());
            }
            Some(rc)
        }));
        toks.push(None);
        for p in toks.windows(2) {
            (&mut self.map[p[0]]).add(p[1].clone());
        }
        self
    }

    /// Generates a collection of tokens from the chain. This operation is O(mn) where m is the
    /// length of the generated collection, and n is the number of possible states from a given
    /// state.
    pub fn generate(&self) -> Vec<Rc<T>> {
        let mut ret = Vec::new();
        let mut curs = None;
        loop {
            curs = self.map[curs].next();
            if curs.is_none() { break }
            ret.push(curs.clone().unwrap());    
        }
        ret
    }

    /// Generates a collection of tokens from the chain, starting with the given token. This
    /// operation is O(mn) where m is the length of the generated collection, and n is the number
    /// of possible states from a given state. This returns an empty vector if the token is not
    /// found.
    pub fn generate_from_token(&self, token: T) -> Vec<Rc<T>> {
        let token = Rc::new(token);
        if !self.map.contains_key(&Some(token.clone())) { return Vec::new() }
        let mut ret = vec![token.clone()];
        let mut curs = Some(token);
        loop {
            curs = self.map[curs].next();
            if curs.is_none() { break }
            ret.push(curs.clone().unwrap());    
        }
        ret
    }

    /// Produces an infinite iterator of generated token collections.
    pub fn iter(&self) -> InfiniteChainIterator<T> {
        InfiniteChainIterator { chain: self }
    }

    /// Produces an iterator for the specified number of generated token collections.
    pub fn iter_for(&self, size: uint) -> SizedChainIterator<T> {
        SizedChainIterator { chain: self, size: size }
    }
}

impl<T: Decodable<Decoder, DecoderError> + Eq + Hash> Chain<T> {
    /// Loads a chain from a JSON file at the specified path.
    pub fn load(path: &Path) -> IoResult<Chain<T>> {
        let mut file = try!(File::open(path));
        let data = try!(file.read_to_string());
        decode(data[]).map_err(|e| IoError {
            kind: InvalidInput,
            desc: "Decoder error",
            detail: Some(e.to_string()),
        })
    }

    /// Loads a chain from a JSON file using a string path.
    pub fn load_utf8(path: &str) -> IoResult<Chain<T>> {
        Chain::load(&Path::new(path))
    }
}

impl<'a, T: Encodable<Encoder<'a>, IoError> + Eq + Hash> Chain<T> {
    /// Saves a chain to a JSON file at the specified path.
    pub fn save(&self, path: &Path) -> IoResult<()> {
        let mut f = File::create(path);
        f.write_str(encode(self)[])
    }

    /// Saves a chain to a JSON file using a string path.
    pub fn save_utf8(&self, path: &str) -> IoResult<()> {
        self.save(&Path::new(path))
    }
}

impl Chain<String> {
    /// Feeds a string of text into the chain.     
    pub fn feed_str(&mut self, string: &str) -> &mut Chain<String> {
        self.feed(string.split_str(" ").map(|s| s.to_owned()).collect())
    }

    /// Feeds a properly formatted file into the chain. This file should be formatted such that
    /// each line is a new sentence. Punctuation may be included if it is desired.
    pub fn feed_file(&mut self, path: &Path) -> &mut Chain<String> {
        let mut reader = BufferedReader::new(File::open(path));
        for line in reader.lines() {
            let line = line.unwrap();
            let words: Vec<_> = line.split([' ', '\t', '\n', '\r'][])
                                    .filter(|word| !word.is_empty())
                                    .collect();
            self.feed(words.iter().map(|&s| s.to_owned()).collect());
        }
        self
    }

    /// Converts the output of generate(...) on a String chain to a single String.
    fn vec_to_string(vec: Vec<Rc<String>>) -> String {
        let mut ret = String::new();
        for s in vec.iter() {
            ret.push_str(s[]);
            ret.push_str(" ");
        }
        let len = ret.len();
        if len > 0 { 
            ret.truncate(len - 1);
        }
        ret
    }

    /// Generates a random string of text.
    pub fn generate_str(&self) -> String { 
        Chain::vec_to_string(self.generate())    
    }

    /// Generates a random string of text starting with the desired token. This returns an empty
    /// string if the token is not found.
    pub fn generate_str_from_token(&self, string: &str) -> String {
        Chain::vec_to_string(self.generate_from_token(string.to_owned()))
    }

    /// Produces an infinite iterator of generated strings.
    pub fn str_iter(&self) -> InfiniteChainStringIterator {
        let vec_to_string: fn(Vec<Rc<String>>) -> String = Chain::vec_to_string;
        self.iter().map(vec_to_string) 
    }

    /// Produces a sized iterator of generated strings.
    pub fn str_iter_for(&self, size: uint) -> SizedChainStringIterator {
        let vec_to_string: fn(Vec<Rc<String>>) -> String = Chain::vec_to_string;
        self.iter_for(size).map(vec_to_string)
    }
}

/// A sized iterator over a Markov chain of strings.
pub type SizedChainStringIterator<'a> =
Map<Vec<Rc<String>>, String, SizedChainIterator<'a, String>, fn(Vec<Rc<String>>) -> String>;

/// A sized iterator over a Markov chain.
pub struct SizedChainIterator<'a, T: Eq + Hash + 'a> {
    chain: &'a Chain<T>,
    size: uint,
}

impl<'a, T: Eq + Hash + 'a> Iterator<Vec<Rc<T>>> for SizedChainIterator<'a, T> {
    fn next(&mut self) -> Option<Vec<Rc<T>>> {
        if self.size > 0 {
            self.size -= 1;
            Some(self.chain.generate())
        } else {
            None
        }
    }

    fn size_hint(&self) -> (uint, Option<uint>) { 
        (self.size, Some(self.size)) 
    }
}


/// An infinite iterator over a Markov chain of strings.
pub type InfiniteChainStringIterator<'a> = 
Map<Vec<Rc<String>>, String, InfiniteChainIterator<'a, String>, fn(Vec<Rc<String>>) -> String>;

/// An infinite iterator over a Markov chain.
pub struct InfiniteChainIterator<'a, T: Eq + Hash + 'a> {
    chain: &'a Chain<T>
}

impl<'a, T: Eq + Hash + 'a> Iterator<Vec<Rc<T>>> for InfiniteChainIterator<'a, T> {
    fn next(&mut self) -> Option<Vec<Rc<T>>> {
        Some(self.chain.generate())
    }
}

/// A collection of states for the Markov chain.
trait States<T: PartialEq> {
    /// Adds a state to this states collection.
    fn add(&mut self, token: Option<Rc<T>>);
    /// Gets the next state from this collection of states.
    fn next(&self) -> Option<Rc<T>>;
}

impl<T: Eq + Hash> States<T> for HashMap<Option<Rc<T>>, uint> {
    fn add(&mut self, token: Option<Rc<T>>) {
        match self.entry(token) {
            Occupied(mut e) => *e.get_mut() += 1,
            Vacant(e) => { e.set(1); },
        }
    }

    fn next(&self) -> Option<Rc<T>> {
        let mut sum = 0;
        for &value in self.values() {
            sum += value;
        }
        let mut rng = task_rng();
        let cap = rng.gen_range(0, sum);
        sum = 0;
        for (key, &value) in self.iter() {
            sum += value;
            if sum > cap {
                return key.clone()
            }
        }
        unreachable!("The random number generator failed.")
    }
}

#[cfg(test)]
mod test {
    use super::Chain;

    #[test]
    fn new() {
        Chain::<u8>::new();
        Chain::<String>::new();
    }

    #[test]
    fn is_empty() {
        let mut chain = Chain::new();
        assert!(chain.is_empty());
        chain.feed(vec![1u, 2u, 3u]);
        assert!(!chain.is_empty());
    }

    #[test]
    fn feed() {
        let mut chain = Chain::new();
        chain.feed(vec![3u, 5u, 10u]).feed(vec![5u, 12u]);
    }

    #[test]
    fn generate() {
        let mut chain = Chain::new();
        chain.feed(vec![3u, 5u, 10u]).feed(vec![5u, 12u]);
        let v = chain.generate().map_in_place(|v| *v);
        assert!([vec![3u, 5u, 10u], vec![3u, 5u, 12u], vec![5u, 10u], vec![5u, 12u]].contains(&v));
    }

    #[test]
    fn generate_from_token() {
        let mut chain = Chain::new();
        chain.feed(vec![3u, 5u, 10u]).feed(vec![5u, 12u]);
        let v = chain.generate_from_token(5u).map_in_place(|v| *v);
        assert!([vec![5u, 10u], vec![5u, 12u]].contains(&v));
    }

    #[test]
    fn generate_from_unfound_token() {
        let mut chain = Chain::new();
        chain.feed(vec![3u, 5u, 10u]).feed(vec![5u, 12u]);
        let v = chain.generate_from_token(9u).map_in_place(|v| *v);
        assert_eq!(v, vec![]);
    }

    #[test]
    fn iter() {    
        let mut chain = Chain::new();
        chain.feed(vec![3u, 5u, 10u]).feed(vec![5u, 12u]);
        assert_eq!(chain.iter().size_hint().1, None);
    }

    #[test]
    fn iter_for() {   
        let mut chain = Chain::new();
        chain.feed(vec![3u, 5u, 10u]).feed(vec![5u, 12u]);
        assert_eq!(chain.iter_for(5).collect::<Vec<_>>().len(), 5);
    }

    #[test]
    fn feed_str() {
        let mut chain = Chain::new();
        chain.feed_str("I like cats and dogs");
    }

    #[test]
    fn generate_str() {
        let mut chain = Chain::new();
        chain.feed_str("I like cats").feed_str("I hate cats");
        let out = chain.generate_str();
        println!("{}", out);
        assert!(["I like cats", "I hate cats"].contains(&out[]));
    }

    #[test]
    fn generate_str_from_token() {
        let mut chain = Chain::new();
        chain.feed_str("I like cats").feed_str("cats are cute");
        assert!(["cats", "cats are cute"].contains(&chain.generate_str_from_token("cats")[]));
    }

    #[test]
    fn generate_str_from_unfound_token() {
        let mut chain = Chain::new();
        chain.feed_str("I like cats").feed_str("cats are cute");
        assert_eq!(chain.generate_str_from_token("test"), "");
    }
    
    #[test]
    fn str_iter() {    
        let mut chain = Chain::new();
        chain.feed_str("I like cats and I like dogs");
        assert_eq!(chain.str_iter().size_hint().1, None);
    }

    #[test]
    fn str_iter_for() {   
        let mut chain = Chain::new();
        chain.feed_str("I like cats and I like dogs");
        assert_eq!(chain.str_iter_for(5).collect::<Vec<_>>().len(), 5);
    }


    #[test]
    fn save() {
        let mut chain = Chain::new();
        chain.feed_str("I like cats and I like dogs");
        chain.save_utf8("save.json").unwrap();
    }

    #[test]
    fn load() {
        let mut chain = Chain::new();
        chain.feed_str("I like cats and I like dogs");
        chain.save_utf8("load.json").unwrap();
        let other_chain: Chain<String> = Chain::load_utf8("load.json").unwrap();
        assert_eq!(other_chain, chain);
    }
}

