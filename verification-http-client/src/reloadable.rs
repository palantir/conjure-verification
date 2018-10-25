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

use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

#[derive(Clone)]
pub struct Reloadable<T>(Arc<AtomicPtr<T>>);

impl<T> Drop for Reloadable<T> {
    fn drop(&mut self) {
        self.take();
    }
}

impl<T> Reloadable<T> {
    pub fn new(value: T) -> Reloadable<T> {
        let reloadable = Reloadable(Arc::new(AtomicPtr::new(ptr::null_mut())));
        reloadable.set(value);
        reloadable
    }

    pub fn set(&self, value: T) {
        unsafe {
            let value = Box::into_raw(Box::new(value));
            let old_value = self.0.swap(value, Ordering::SeqCst);
            if !old_value.is_null() {
                Box::from_raw(old_value);
            }
        }
    }

    pub fn take(&self) -> Option<T> {
        unsafe {
            let value = self.0.swap(ptr::null_mut(), Ordering::SeqCst);
            if value.is_null() {
                None
            } else {
                Some(*Box::from_raw(value))
            }
        }
    }
}
