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
use std::time::Duration;

pub(crate) struct BackoffIterator {
    max_num_retries: u32,
    retries: u32,
    backoff_slot_size: Duration,
}

impl BackoffIterator {
    pub fn new(max_num_retries: u32, backoff_slot_size: Duration) -> BackoffIterator {
        BackoffIterator {
            max_num_retries,
            retries: 0,
            backoff_slot_size,
        }
    }
}

impl Iterator for BackoffIterator {
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        self.retries += 1;
        if self.retries > self.max_num_retries {
            return None;
        }

        let scale = 1 << self.retries;
        let max = self.backoff_slot_size * scale;

        Some(rand::thread_rng().gen_range(Duration::from_secs(0), max))
    }
}
