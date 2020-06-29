//! Common utilities

use std::collections::{hash_map::Entry, HashMap};

/// Classifies lines, converting lines into unique `u64`s for quicker comparison
#[derive(Default)]
pub struct Classifier<'a> {
    next_id: u64,
    unique_ids: HashMap<&'a str, u64>,
}

impl<'a> Classifier<'a> {
    fn classify(&mut self, record: &'a str) -> u64 {
        match self.unique_ids.entry(record) {
            Entry::Occupied(o) => *o.get(),
            Entry::Vacant(v) => {
                let id = self.next_id;
                self.next_id += 1;
                *v.insert(id)
            }
        }
    }

    pub fn classify_lines(&mut self, text: &'a str) -> (Vec<&'a str>, Vec<u64>) {
        LineIter::new(text)
            .map(|line| (line, self.classify(&line)))
            .unzip()
    }
}

/// Iterator over the lines of a string, including the `\n` character.
pub struct LineIter<'a>(&'a str);

impl<'a> LineIter<'a> {
    pub fn new(text: &'a str) -> Self {
        Self(text)
    }
}

impl<'a> Iterator for LineIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }

        let end = if let Some(idx) = self.0.find('\n') {
            idx + 1
        } else {
            self.0.len()
        };

        let (line, remaining) = self.0.split_at(end);
        self.0 = remaining;
        Some(line)
    }
}
