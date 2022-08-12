// (c) Copyright 2018 Palantir Technologies Inc. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use rand::{self, Rng};
use std::sync::atomic::{AtomicUsize, Ordering};
use url::Url;

pub(crate) struct Node {
    pub url: Url,
}

pub(crate) struct NodeSelector {
    nodes: Vec<Node>,
    idx: AtomicUsize,
}

impl NodeSelector {
    pub fn new(urls: &[Url]) -> NodeSelector {
        let mut nodes = urls
            .iter()
            .map(|url| {
                // normalize by stripping a trailing `/` if present
                let mut url = url.clone();
                url.path_segments_mut().unwrap().pop_if_empty();
                Node { url }
            })
            .collect::<Vec<_>>();

        // randomize node order so all services don't hotspot on one node, but we want deterministic tests
        if cfg!(not(test)) {
            rand::thread_rng().shuffle(&mut nodes);
        }

        NodeSelector {
            nodes,
            idx: AtomicUsize::new(0),
        }
    }

    pub fn iter(&self) -> Option<NodeIter> {
        if self.nodes.is_empty() {
            None
        } else {
            Some(NodeIter {
                nodes: self,
                idx: self.idx.load(Ordering::SeqCst),
            })
        }
    }
}

pub(crate) struct NodeIter<'a> {
    nodes: &'a NodeSelector,
    idx: usize,
}

impl<'a> NodeIter<'a> {
    pub fn get(&self) -> &'a Node {
        &self.nodes.nodes[self.idx]
    }

    pub fn next(&mut self) -> &'a Node {
        self.idx = (self.idx + 1) % self.nodes.nodes.len();
        self.get()
    }

    pub fn set_default(&self) {
        self.nodes.idx.store(self.idx, Ordering::SeqCst);
    }
}
