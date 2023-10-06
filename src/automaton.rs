use core::panic;
use std::{rc::{Rc, Weak}, collections::HashMap, cell::RefCell, fmt::Display};

/// Holds the AcAutomatonNode trie
#[derive(Debug)]
pub struct AcAutomaton {
    root: Rc<RefCell<AcAutomatonNode>>,
    state: Rc<RefCell<AcAutomatonNode>> // Cursor pointing to the current state
}

impl Display for AcAutomaton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.root.borrow())
    }
}

/// A simple trie implementation with minimal features
#[derive(Debug)]
struct AcAutomatonNode {
    depth: usize,
    children: HashMap<u8, Rc<RefCell<AcAutomatonNode>>>,
    suffix_link: Weak<RefCell<AcAutomatonNode>>,
    output_link: Weak<RefCell<AcAutomatonNode>>,
    is_word: bool, // If true, the word ending here belongs to the dictionnary
}

impl Display for AcAutomatonNode {
    /// Used only for debugging
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:[{}]",
            (if self.is_word { "O" } else { "X" }),
            //self.suffix_link.upgrade().map(|link| link.borrow().id.clone()).unwrap_or("N/A".to_string()),
            //self.output_link.upgrade().map(|link| link.borrow().id.clone()).unwrap_or("N/A".to_string()),
            self.children.iter().fold(String::new(), |mut acc, (val, node)| {
                if acc.len() != 0 {
                    acc.push('\n');
                }
                acc.push_str(&format!("{} => {}", *val as char, node.borrow().to_string()));
                acc
            })
        )
    }
}

impl AcAutomaton {
    pub fn new(words: Vec<&[u8]>) -> Self {
        let root = AcAutomatonNode {
            depth: 0,
            children: HashMap::new(),
            suffix_link: Weak::new(), // In the end, the only node not having suffix_link should be root
            output_link: Weak::new(),
            is_word: false,
        };
        let root_rc = Rc::new(RefCell::new(root));
        let mut ac = AcAutomaton {
            root: Rc::clone(&root_rc),
            state: root_rc,
        };
        for word in words.into_iter() {
            ac.add_word(word);
        }
        ac.breadth_first_walk();
        ac
    }

    fn add_word(&mut self, word: &[u8]) {
        self.root.borrow_mut().add_word(&word);
    }

    /// Breadth-first calculating suffix links for each node
    fn breadth_first_walk(&mut self) {
        let root_ref = Rc::downgrade(&self.root);
        for (_, root_child_node) in self.root.borrow_mut().children.iter() {
            // First level children suffix links are always pointing to root
            root_child_node.borrow_mut().suffix_link = Weak::clone(&root_ref);
        }
        let mut to_walk: Vec<Rc<RefCell<AcAutomatonNode>>> = self.root.borrow_mut().children.values().map(|v| Rc::clone(v)).collect();
        while to_walk.len() > 0 {
            // Each iteration is a N+1 depth level of children. Walking through children appends RCs of their own children for the next iteration
            let mut new_to_walk: Vec<Rc<RefCell<AcAutomatonNode>>> = Vec::new();
            for node_ref in to_walk {
                let mut next_level_children = node_ref.borrow().calculate_children_links();
                if next_level_children.len() > 0 {
                    new_to_walk.append(&mut next_level_children);
                }
            }
            to_walk = new_to_walk;
        }
    }

    /// Advances the state
    #[inline(always)]
    pub fn next_state(&mut self, char: &u8) {
        self.state = AcAutomatonNode::find_next_state(Rc::clone(&self.state), char)
    }

    pub fn is_state_root(&self) -> bool {
        self.state.borrow().suffix_link.upgrade().is_none()
    }

    pub fn is_state_word(&self) -> bool {
        self.state.borrow().is_word
    }

    /// Reset state to point at root
    pub fn reset_state(&mut self) {
        self.state = Rc::clone(&self.root)
    }

    pub fn state_depth(&self) -> usize {
        self.state.borrow().depth
    }
}

impl AcAutomatonNode {
    fn add_word(&mut self, word: &[u8]) {
        if word.len() == 0 {
            self.is_word = true;
            return;
        }
        let (first, remaining_word) = word.split_first().unwrap(); // word is not empty
        let child = self.children.entry(*first).or_insert(Rc::new(RefCell::new(AcAutomatonNode {
            depth: self.depth + 1,
            children: HashMap::new(),
            is_word: false,
            output_link: Weak::new(),
            suffix_link: Weak::new(),
        })));
        child.borrow_mut().add_word(remaining_word);
    }

    /// Calculates the suffix and output links for all children of the given node. Assumes that all N-1 nodes' suffix links are already determined
    /// Returns the list of its own children for next level processing
    fn calculate_children_links(&self) -> Vec<Rc<RefCell<AcAutomatonNode>>> {
        for (val, child) in &self.children {
            match self.suffix_link.upgrade() {
                Some(mut ancestor_link) => {
                    'suffix_search: loop {
                        // First iteration : ancestor is direct parent. If not found, follow parent's suffix link to next ancestor, and repeat search
                        match Rc::clone(&ancestor_link).borrow().children.get(val) {
                            Some(step_val) => {
                                child.borrow_mut().suffix_link = Rc::downgrade(step_val);
                                break 'suffix_search;
                            },
                            None => {
                                match Rc::clone(&ancestor_link).borrow().suffix_link.upgrade() {
                                    Some(prev_ancestor_link) => {
                                        ancestor_link = prev_ancestor_link;
                                    },
                                    None => {
                                        // Root is reached
                                        child.borrow_mut().suffix_link = Rc::downgrade(&ancestor_link);
                                        break 'suffix_search;
                                    }
                                }
                            }
                        }
                    }
                    // Output link is either suffix list itself if it is a word, or that suffix's output link
                    let suffix_link = child.borrow().suffix_link.upgrade().unwrap();
                    if suffix_link.borrow().is_word {
                        child.borrow_mut().output_link = Rc::downgrade(&suffix_link);
                    } else {
                        child.borrow_mut().output_link = Weak::clone(&suffix_link.borrow().output_link);
                    }
                },
                None => {
                    panic!("Logic error : current node's suffix link must always exist");
                }
            }
        }
        self.children.values().map(|v| Rc::clone(v)).collect()
    }

    /// Recursive function to find the next state by following suffix links and examining their children
    #[inline(always)]
    fn find_next_state(this: Rc<RefCell<Self>>, char: &u8) -> Rc<RefCell<AcAutomatonNode>> {
        let this_borrowed = this.borrow();
        match this_borrowed.children.get(char) {
            Some(next) => Rc::clone(next),
            None => match this_borrowed.suffix_link.upgrade() {
                Some(suffix_link) => Self::find_next_state(suffix_link, char),
                None => {
                    // Root is the only node not having a suffix link
                    Rc::clone(&this)
                },
            }
        }
    }
}